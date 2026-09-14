import { buttonVariants, cn, Section } from "@allsource/ui";
import { ArrowRight } from "lucide-react";
import Link from "next/link";
import BlogCard from "@/components/blog-card";
import { getBlogPosts } from "@/lib/blog";

export default async function BlogSection() {
  const allPosts = await getBlogPosts();

  const articles = allPosts.sort((a, b) => b.publishedAt.localeCompare(a.publishedAt)).slice(0, 3);

  return (
    <Section title="Recent product and engineering notes" subtitle="From the AllSource team">
      <div className="grid grid-cols-1 gap-8 md:grid-cols-2 lg:grid-cols-3">
        {articles.map((data) => (
          <BlogCard key={data.slug} data={data} />
        ))}
      </div>
      <div className="mt-8 flex justify-center">
        <Link href="/blog" className={cn(buttonVariants({ variant: "outline" }), "gap-2")}>
          View all articles
          <ArrowRight className="h-4 w-4" aria-hidden="true" />
        </Link>
      </div>
    </Section>
  );
}
