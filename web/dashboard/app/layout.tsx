import "./globals.css";
import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "OpenPay clearing desk",
  description: "Self-hosted payment orchestration dashboard (sandbox)",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="min-h-screen font-sans antialiased">{children}</body>
    </html>
  );
}
