import type { Metadata } from "next";
import data from "./worksheet.json";
import "./worksheet.css";

export const metadata: Metadata = {
  title: { absolute: data.title },
  description: data.description,
  alternates: { canonical: data.canonical },
  openGraph: {
    title: data.title,
    description: data.description,
    url: data.canonical,
    type: "article",
    locale: "en_GB",
  },
  twitter: { card: "summary", title: data.title, description: data.description },
  robots: { index: true, follow: true },
};

export default function ProductWorksheetPage() {
  return (
    <>
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: Static repository JSON-LD, escaped before embedding.
        dangerouslySetInnerHTML={{ __html: JSON.stringify(data.schema).replace(/</g, "\\u003c") }}
      />
      {/* biome-ignore lint/security/noDangerouslySetInnerHtml: Reviewed, escaped static worksheet HTML; no user or runtime input. */}
      <div dangerouslySetInnerHTML={{ __html: data.html }} />
    </>
  );
}
