import type { Metadata } from "next";
import data from "./worksheet.json";
import "./worksheet.css";

export const metadata: Metadata = {
  title: { absolute: data.title },
  description: data.description,
  alternates: { canonical: data.canonical },
  openGraph: { title: data.title, description: data.description, url: data.canonical, type: "article", locale: "en_GB" },
  twitter: { card: "summary", title: data.title, description: data.description },
  robots: { index: true, follow: true },
};

export default function ProductWorksheetPage() {
  return <>
    {/* Static, reviewed repository content; no user or runtime input. */}
    <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(data.schema).replace(/</g, "\\u003c") }} />
    <div dangerouslySetInnerHTML={{ __html: data.html }} />
  </>;
}
