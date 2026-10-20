import { type ClassValue, clsx } from "clsx";
import type { Metadata } from "next";
import { twMerge } from "tailwind-merge";
import { siteConfig } from "@/lib/config";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function absoluteUrl(path: string) {
  return `${process.env.NEXT_PUBLIC_APP_URL || siteConfig.url}${path}`;
}

// Twitter handle for card attribution, derived from siteConfig so the @ form
// stays in lockstep with the bare handle used elsewhere.
const TWITTER_HANDLE = `@${siteConfig.twitterHandle}`;

export function constructMetadata({
  title = siteConfig.name,
  description = siteConfig.description,
  image = absoluteUrl("/og"),
  imageAlt,
  canonical,
  type = "website",
  publishedTime,
  modifiedTime,
  authors,
  section,
  ...props
}: {
  title?: string;
  description?: string;
  image?: string;
  imageAlt?: string;
  canonical?: string;
  type?: "website" | "article";
  publishedTime?: string;
  modifiedTime?: string;
  authors?: string[];
  section?: string;
  [key: string]: Metadata[keyof Metadata];
}): Metadata {
  const url = canonical ? `${siteConfig.url}${canonical}` : siteConfig.url;
  const article =
    type === "article"
      ? {
          type: "article" as const,
          ...(publishedTime && { publishedTime }),
          ...(modifiedTime && { modifiedTime }),
          ...(authors?.length && { authors }),
          ...(section && { section }),
        }
      : { type: "website" as const };

  return {
    title,
    description: description || siteConfig.description,
    keywords: siteConfig.keywords,
    openGraph: {
      title,
      description,
      url,
      siteName: siteConfig.productName,
      images: [
        {
          url: image,
          width: 1200,
          height: 630,
          alt: imageAlt || title,
        },
      ],
      locale: "en_US",
      ...article,
    },
    twitter: {
      card: "summary_large_image",
      title,
      description,
      images: [image],
      site: TWITTER_HANDLE,
      creator: TWITTER_HANDLE,
    },
    icons: "/favicon.ico",
    metadataBase: new URL(siteConfig.url),
    ...(canonical && {
      alternates: {
        canonical: canonical,
      },
    }),
    authors: [
      {
        name: siteConfig.name,
        url: siteConfig.url,
      },
    ],
    ...props,
  };
}

/**
 * Clamps a meta description to `limit` characters on a word boundary.
 *
 * 160 is where Google truncates a desktop snippet. The text stays indexed
 * either way, so this protects the snippet, not the ranking.
 */
export function clampDescription(text: string, limit = 160): string {
  const collapsed = text.replace(/\s+/g, " ").trim();
  if (collapsed.length <= limit) return collapsed;
  const cut = collapsed.slice(0, limit - 1);
  const lastSpace = cut.lastIndexOf(" ");
  return `${(lastSpace > limit * 0.6 ? cut.slice(0, lastSpace) : cut).replace(/[\s,.;:—-]+$/, "")}…`;
}

export function formatDate(date: string) {
  const currentDate = Date.now();
  const normalizedDate = date.includes("T") ? date : `${date}T00:00:00`;
  const targetDate = new Date(normalizedDate).getTime();
  const timeDifference = Math.abs(currentDate - targetDate);
  const daysAgo = Math.floor(timeDifference / (1000 * 60 * 60 * 24));

  const fullDate = new Date(normalizedDate).toLocaleString("en-us", {
    month: "long",
    day: "numeric",
    year: "numeric",
  });

  if (daysAgo < 1) {
    return "Today";
  }
  if (daysAgo < 7) {
    return `${fullDate} (${daysAgo}d ago)`;
  }
  if (daysAgo < 30) {
    const weeksAgo = Math.floor(daysAgo / 7);
    return `${fullDate} (${weeksAgo}w ago)`;
  }
  if (daysAgo < 365) {
    const monthsAgo = Math.floor(daysAgo / 30);
    return `${fullDate} (${monthsAgo}mo ago)`;
  }
  const yearsAgo = Math.floor(daysAgo / 365);
  return `${fullDate} (${yearsAgo}y ago)`;
}
