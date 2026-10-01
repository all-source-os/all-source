import type { Metadata } from "next";
import Image from "next/image";
import { notFound } from "next/navigation";
import { Suspense } from "react";
import Author from "@/components/blog-author";
import CtaSection from "@/components/sections/cta";
import { BLOG_CATEGORIES, getPost } from "@/lib/blog";
import { siteConfig } from "@/lib/config";
import { blogPostingSchema, breadcrumbSchema } from "@/lib/structured-data";
import { constructMetadata, formatDate } from "@/lib/utils";

function categoryLabel(category?: string) {
  return BLOG_CATEGORIES.find((c) => c.value === category)?.label || category;
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata | undefined> {
  const { slug } = await params;
  const post = await getPost(slug);
  if (!post) {
    return undefined;
  }
  const { title, publishedAt, updatedAt, summary, imageUrl, author, category } = post.metadata;
  const section = categoryLabel(category);

  return constructMetadata({
    title,
    description: summary,
    image: imageUrl,
    imageAlt: title,
    canonical: `/blog/${post.slug}`,
    type: "article",
    publishedTime: publishedAt,
    modifiedTime: updatedAt || publishedAt,
    ...(author && { authors: [author] }),
    ...(section && { section }),
  });
}

export default async function Blog({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const post = await getPost(slug);
  if (!post) {
    notFound();
  }
  const section = categoryLabel(post.metadata.category);
  const blogPosting = blogPostingSchema({
    title: post.metadata.title,
    description: post.metadata.summary,
    slug: post.slug,
    image: post.metadata.imageUrl,
    datePublished: post.metadata.publishedAt,
    dateModified: post.metadata.updatedAt || post.metadata.publishedAt,
    author: post.metadata.author,
    section,
    ...(section && { keywords: [section] }),
    wordCount: post.metadata.wordCount,
  });
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "Blog", path: "/blog" },
    { name: post.metadata.title, path: `/blog/${post.slug}` },
  ]);

  return (
    <section id="blog">
      <script
        type="application/ld+json"
        suppressHydrationWarning
        // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
        dangerouslySetInnerHTML={{ __html: JSON.stringify(blogPosting) }}
      />
      <script
        type="application/ld+json"
        suppressHydrationWarning
        // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
        dangerouslySetInnerHTML={{ __html: JSON.stringify(breadcrumb) }}
      />
      <div className="mx-auto w-full max-w-[800px] px-4 sm:px-6 lg:px-8 space-y-4 my-12">
        <Suspense
          fallback={<div className="mb-8 w-full h-64 bg-gray-200 animate-pulse rounded-lg" />}
        >
          {post.metadata.image && (
            <div className="mb-8">
              <Image
                width={1920}
                height={1080}
                src={post.metadata.image}
                alt={post.metadata.title}
                className="w-full h-auto rounded-lg border shadow-md"
              />
            </div>
          )}
        </Suspense>
        <div className="flex flex-col">
          <h1 className="title font-medium text-3xl tracking-tighter">{post.metadata.title}</h1>
        </div>
        <div className="flex justify-between items-center text-sm">
          <Suspense fallback={<p className="h-5" />}>
            <div className="flex items-center space-x-2">
              <time dateTime={post.metadata.publishedAt} className="text-sm text-gray-500">
                {formatDate(post.metadata.publishedAt)}
              </time>
            </div>
          </Suspense>
        </div>
        <div className="flex items-center space-x-2">
          <Author
            twitterUsername={siteConfig.twitterHandle}
            name={post.metadata.author}
            image={"/author.jpg"}
          />
        </div>
        <article
          className="prose dark:prose-invert mx-auto max-w-full [&_table]:table-fixed [&_td]:break-words [&_th]:break-words"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: Server-rendered markdown content is sanitized
          dangerouslySetInnerHTML={{ __html: post.source }}
        />
      </div>
      <CtaSection />
    </section>
  );
}
