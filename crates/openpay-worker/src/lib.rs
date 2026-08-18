use std::net::IpAddr;
use std::time::Duration;

use ipnet::IpNet;
use rand::Rng;
use reqwest::Client;
use serde_json::Value;
use time::{Duration as TimeDuration, OffsetDateTime};
use tracing::{info, warn};
use url::Url;

use openpay_application::{
    PaymentRepository, PaymentService, SystemClock, WebhookRepository, reconcile_stale_attempts,
};
use openpay_config::AppConfig;
use openpay_connectors::ConnectorRegistry;
use openpay_crypto::sign_webhook;
use openpay_domain::{
    DeliveryStatus, EventId, WebhookDelivery, WebhookDeliveryId, WebhookEndpoint,
};
use openpay_persistence::PgStore;

const PRIVATE_NETS: &[&str] = &[
    "0.0.0.0/8",
    "10.0.0.0/8",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "224.0.0.0/4",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];

const METADATA_HOSTS: &[&str] = &[
    "metadata.google.internal",
    "metadata.google.com",
    "169.254.169.254",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookDnsPolicy {
    AllowPrivateResolution,
    RejectPrivateResolution,
}

pub fn is_blocked_ip(ip: IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    if is_metadata_ip(ip) {
        return true;
    }
    match ip {
        IpAddr::V4(v4) if v4.is_private() || v4.is_link_local() => true,
        _ => PRIVATE_NETS
            .iter()
            .filter_map(|c| c.parse::<IpNet>().ok())
            .any(|net| net.contains(&ip)),
    }
}

pub fn is_metadata_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.octets() == [169, 254, 169, 254],
        _ => false,
    }
}

pub fn is_metadata_host(host: &str) -> bool {
    let host = host.trim().trim_matches(|c| c == '[' || c == ']');
    METADATA_HOSTS
        .iter()
        .any(|blocked| host.eq_ignore_ascii_case(blocked))
}

pub fn host_in_allowlist(host: &str, allowlist: &[String]) -> bool {
    allowlist
        .iter()
        .any(|entry| host.eq_ignore_ascii_case(entry.trim()))
}

/// Allowlist matches hostnames only. Literal IPs never bypass RFC1918 checks.
/// Metadata hosts/IPs are never allowed, even if listed.
pub fn webhook_dns_policy(host: &str, allowlist: &[String]) -> Result<WebhookDnsPolicy, String> {
    if is_metadata_host(host) {
        return Err("metadata endpoint blocked".into());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_metadata_ip(ip) || is_blocked_ip(ip) {
            return Err("private IP blocked".into());
        }
        if !allowlist.is_empty() && !host_in_allowlist(host, allowlist) {
            return Err("host not in allowlist".into());
        }
        return Ok(WebhookDnsPolicy::RejectPrivateResolution);
    }
    if !allowlist.is_empty() && !host_in_allowlist(host, allowlist) {
        return Err("host not in allowlist".into());
    }
    if host_in_allowlist(host, allowlist) {
        Ok(WebhookDnsPolicy::AllowPrivateResolution)
    } else {
        Ok(WebhookDnsPolicy::RejectPrivateResolution)
    }
}

pub async fn assert_safe_webhook_url(url: &str, allowlist: &[String]) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("unsupported scheme".into());
    }
    let host = parsed.host_str().ok_or("missing host")?;
    let policy = webhook_dns_policy(host, allowlist)?;
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    let lookup = tokio::net::lookup_host((host, parsed.port_or_known_default().unwrap_or(80)))
        .await
        .map_err(|e| e.to_string())?;
    for sa in lookup {
        if is_metadata_ip(sa.ip()) {
            return Err("metadata endpoint blocked".into());
        }
        if is_blocked_ip(sa.ip()) && policy != WebhookDnsPolicy::AllowPrivateResolution {
            return Err(format!("resolved private IP {}", sa.ip()));
        }
    }
    Ok(())
}

pub struct WorkerRuntime {
    pub store: PgStore,
    pub config: AppConfig,
    pub http: Client,
    pub payments:
        PaymentService<PgStore, PgStore, PgStore, PgStore, PgStore, PgStore, PgStore, SystemClock>,
    pub connectors: ConnectorRegistry,
}

impl WorkerRuntime {
    pub fn new(
        store: PgStore,
        config: AppConfig,
        connectors: ConnectorRegistry,
    ) -> Result<Self, reqwest::Error> {
        let http = Client::builder()
            .timeout(Duration::from_millis(config.webhook_timeout_ms))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let payments = PaymentService {
            payments: store.clone(),
            merchants: store.clone(),
            audit: store.clone(),
            outbox: store.clone(),
            routing: store.clone(),
            connectors: store.clone(),
            nonces: store.clone(),
            clock: SystemClock,
            qr_secret: config.qr_signing_secret.as_bytes().to_vec(),
            api_base_url: config.api_base_url.clone(),
            wallet_base_url: config.wallet_base_url.clone(),
            qr_ttl_seconds: config.qr_ttl_seconds,
        };
        Ok(Self {
            store,
            config,
            http,
            payments,
            connectors,
        })
    }

    pub async fn tick(&self) -> Result<(), String> {
        let expired = self
            .payments
            .expire_stale_payments(50)
            .await
            .map_err(|e| e.to_string())?;
        if expired > 0 {
            info!(count = expired, "expired stale payments");
            metrics::counter!("openpay_payments_expired").increment(expired as u64);
        }
        self.drain_outbox().await?;
        self.deliver_webhooks().await?;
        let n = reconcile_stale_attempts(&self.payments, &self.connectors, 25)
            .await
            .map_err(|e| e.to_string())?;
        if n > 0 {
            info!(count = n, "reconciled payments");
            metrics::counter!("openpay_reconciled_payments").increment(n as u64);
        }
        if let Ok(stuck) = PaymentRepository::list_reconcilable_payments(&self.store, 500).await {
            metrics::gauge!("openpay_payments_processing").set(stuck.len() as f64);
        }
        Ok(())
    }

    pub async fn drain_outbox(&self) -> Result<(), String> {
        let pending = openpay_application::OutboxRepository::fetch_pending(&self.store, 50)
            .await
            .map_err(|e| e.to_string())?;
        for record in pending {
            let merchant_id = record
                .payload
                .get("data")
                .and_then(|d| d.get("merchant_id"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok());
            if let Some(merchant_id) = merchant_id {
                let endpoints = self
                    .store
                    .list_endpoints(record.tenant_id, merchant_id)
                    .await
                    .map_err(|e| e.to_string())?;
                for endpoint in endpoints {
                    if !endpoint.event_types.is_empty()
                        && !endpoint.event_types.iter().any(|t| t == &record.event_type)
                    {
                        continue;
                    }
                    if endpoint.failure_count >= 20 {
                        warn!(endpoint = %endpoint.url, "circuit open, skipping webhook");
                        continue;
                    }
                    let delivery = WebhookDelivery {
                        id: WebhookDeliveryId::new(),
                        webhook_endpoint_id: endpoint.id,
                        event_id: record.id.parse().unwrap_or_else(|_| EventId::new()),
                        payload_version: "2026-08-18".into(),
                        status: DeliveryStatus::Pending,
                        attempt_count: 0,
                        next_retry_at: Some(OffsetDateTime::now_utc()),
                        response_code: None,
                        last_error_safe: None,
                        created_at: OffsetDateTime::now_utc(),
                        updated_at: OffsetDateTime::now_utc(),
                    };
                    self.store
                        .insert_delivery_with_payload(&delivery, &record.payload)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            openpay_application::OutboxRepository::mark_published(&self.store, &record.id)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub async fn deliver_webhooks(&self) -> Result<(), String> {
        let jobs = self
            .store
            .pending_deliveries_full(25)
            .await
            .map_err(|e| e.to_string())?;
        for (mut delivery, payload, endpoint) in jobs {
            let endpoint_id = endpoint.id;
            match self.send_one(&endpoint, &payload).await {
                Ok(code) => {
                    delivery.status = DeliveryStatus::Delivered;
                    delivery.response_code = Some(code as i32);
                    delivery.updated_at = OffsetDateTime::now_utc();
                    let _ = self
                        .store
                        .record_webhook_endpoint_result(endpoint_id, true)
                        .await;
                    metrics::counter!("openpay_webhook_delivered").increment(1);
                    info!(id = %delivery.id, code, "webhook delivered");
                }
                Err(err) => {
                    delivery.attempt_count += 1;
                    delivery.response_code = err.1;
                    delivery.last_error_safe = Some(err.0.clone());
                    delivery.updated_at = OffsetDateTime::now_utc();
                    let _ = self
                        .store
                        .record_webhook_endpoint_result(endpoint_id, false)
                        .await;
                    metrics::counter!("openpay_webhook_failed").increment(1);
                    if delivery.attempt_count as u32 >= self.config.webhook_max_attempts {
                        delivery.status = DeliveryStatus::DeadLettered;
                        metrics::counter!("openpay_webhook_dead_lettered").increment(1);
                    } else {
                        delivery.status = DeliveryStatus::Retrying;
                        let backoff = exponential_backoff(delivery.attempt_count);
                        delivery.next_retry_at = Some(OffsetDateTime::now_utc() + backoff);
                    }
                    warn!(id = %delivery.id, error = %err.0, "webhook delivery failed");
                }
            }
            WebhookRepository::update_delivery(&self.store, delivery)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn send_one(
        &self,
        endpoint: &WebhookEndpoint,
        payload: &Value,
    ) -> Result<u16, (String, Option<i32>)> {
        if let Err(e) =
            assert_safe_webhook_url(&endpoint.url, &self.config.webhook_url_allowlist).await
        {
            return Err((format!("ssrf:{e}"), None));
        }
        let body = serde_json::to_vec(payload).map_err(|e| (e.to_string(), None))?;
        let ts = OffsetDateTime::now_utc().unix_timestamp();
        let sig = sign_webhook(self.config.webhook_signing_secret.as_bytes(), ts, &body)
            .map_err(|e| (e.to_string(), None))?;
        let response = self
            .http
            .post(&endpoint.url)
            .header("content-type", "application/json")
            .header("OpenPay-Signature", sig)
            .header(
                "OpenPay-Event",
                payload
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
            )
            .body(body)
            .send()
            .await
            .map_err(|e| (e.to_string(), None))?;
        let status = response.status();
        if status.is_success() {
            Ok(status.as_u16())
        } else {
            Err((
                format!("http {}", status.as_u16()),
                Some(status.as_u16() as i32),
            ))
        }
    }
}

fn exponential_backoff(attempt: i32) -> TimeDuration {
    let base = 2f64.powi(attempt.min(10)).max(1.0);
    let jitter: f64 = rand::thread_rng().gen_range(0.0..0.5);
    let secs = (base + jitter).min(3600.0);
    TimeDuration::seconds(secs as i64)
}

pub async fn run_loop(runtime: WorkerRuntime) {
    loop {
        if let Err(err) = runtime.tick().await {
            warn!(error = %err, "worker tick failed");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn allow(hosts: &[&str]) -> Vec<String> {
        hosts.iter().map(|h| (*h).to_string()).collect()
    }

    #[test]
    fn private_and_metadata_ips_are_blocked() {
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))));
        assert!(is_blocked_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
        assert!(!is_blocked_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn allowlist_does_not_bypass_literal_private_ip() {
        let err = webhook_dns_policy("10.0.0.5", &allow(&["10.0.0.5"])).unwrap_err();
        assert!(err.contains("private IP"));
        let err = webhook_dns_policy("127.0.0.1", &allow(&["127.0.0.1"])).unwrap_err();
        assert!(err.contains("private IP"));
    }

    #[test]
    fn metadata_blocked_even_if_allowlisted() {
        let hosts = allow(&["metadata.google.internal", "169.254.169.254"]);
        assert!(
            webhook_dns_policy("metadata.google.internal", &hosts)
                .unwrap_err()
                .contains("metadata")
        );
        assert!(
            webhook_dns_policy("169.254.169.254", &hosts)
                .unwrap_err()
                .contains("metadata")
        );
    }

    #[test]
    fn allowlisted_hostname_demo_merchant_may_resolve_private() {
        let policy = webhook_dns_policy("demo-merchant", &allow(&["demo-merchant"])).unwrap();
        assert_eq!(policy, WebhookDnsPolicy::AllowPrivateResolution);
    }

    #[test]
    fn non_allowlisted_hostname_rejects_private_resolution() {
        let policy = webhook_dns_policy("evil.internal", &[]).unwrap();
        assert_eq!(policy, WebhookDnsPolicy::RejectPrivateResolution);
        assert!(webhook_dns_policy("evil.internal", &allow(&["demo-merchant"])).is_err());
    }

    #[tokio::test]
    async fn private_literal_url_blocked_even_with_allowlist() {
        let err = assert_safe_webhook_url("http://127.0.0.1/hook", &allow(&["127.0.0.1"]))
            .await
            .unwrap_err();
        assert!(err.contains("private IP"));
    }

    #[tokio::test]
    async fn metadata_url_blocked() {
        let err = assert_safe_webhook_url(
            "http://169.254.169.254/latest/meta-data",
            &allow(&["169.254.169.254"]),
        )
        .await
        .unwrap_err();
        assert!(err.contains("metadata") || err.contains("private IP"));
    }

    #[tokio::test]
    async fn allowlisted_localhost_hostname_is_allowed() {
        assert_safe_webhook_url(
            "http://localhost:3002/webhooks/openpay",
            &allow(&["localhost"]),
        )
        .await
        .expect("allowlisted hostname may resolve loopback");
    }
}
