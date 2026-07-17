import type { Metadata } from "next";
import { headers } from "next/headers";
import { IBM_Plex_Mono, Manrope } from "next/font/google";
import "./globals.css";

const manrope = Manrope({
  variable: "--font-manrope",
  subsets: ["latin"],
});

const plexMono = IBM_Plex_Mono({
  variable: "--font-plex-mono",
  subsets: ["latin"],
  weight: ["400", "500", "600"],
});

export async function generateMetadata(): Promise<Metadata> {
  const headerList = await headers();
  const host = headerList.get("x-forwarded-host") ?? headerList.get("host");
  const protocol =
    headerList.get("x-forwarded-proto") ??
    (host?.startsWith("localhost") ? "http" : "https");
  const base = new URL(`${protocol}://${host ?? "localhost:3000"}`);

  return {
    metadataBase: base,
    title: "Apex Exec — Move the Apex inner loop off the org",
    description:
      "A deterministic, org-independent Apex compiler and runtime for fast local feedback and ordinary CI workers.",
    openGraph: {
      type: "website",
      title: "Move the Apex inner loop off the org.",
      description:
        "Deterministic compile, test, and debug feedback for Salesforce engineering teams.",
      siteName: "Apex Exec",
      images: [
        {
          url: new URL("/og.png", base),
          width: 1200,
          height: 630,
          alt: "Apex Exec — local-first Apex development",
        },
      ],
    },
    twitter: {
      card: "summary_large_image",
      title: "Apex Exec — local-first Apex development",
      description:
        "Move routine Apex compile and test feedback onto developer machines and ordinary CI.",
      images: [new URL("/og.png", base)],
    },
  };
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className={`${manrope.variable} ${plexMono.variable}`}>
        {children}
      </body>
    </html>
  );
}
