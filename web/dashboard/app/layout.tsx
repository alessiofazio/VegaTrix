import "./globals.css";
import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "OpenPay desk operatore",
  description: "Configurazione tenant, chiavi, webhook e laboratorio sandbox",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="it">
      <body className="min-h-screen font-sans antialiased">{children}</body>
    </html>
  );
}
