import Image from "next/image";
import Link from "next/link";
import { BLOG_CATEGORIES, type Post } from "@/lib/blog";
import { formatDate } from "@/lib/utils";

export default function BlogCard({ data, priority }: { data: Post; priority?: boolean }) {
  const categoryLabel = data.category
    ? BLOG_CATEGORIES.find((c) => c.value === data.category)?.label
    : null;

  return (
    <Link href={`/blog/${data.slug}`} className="block">
      <div className="bg-background rounded-lg p-4 mb-4 border hover:shadow-sm transition-shadow duration-200">
        {data.image && (
          <Image
            className="rounded-t-lg object-cover border mb-4"
            src={data.image}
            width={1200}
            height={630}
            alt={data.title}
            priority={priority}
            sizes="(min-width: 1024px) 33vw, (min-width: 768px) 50vw, 100vw"
          />
        )}
        {!data.image && <div className="bg-gray-200 h-[180px] mb-4 rounded" />}
        <div className="flex items-center gap-2 mb-2">
          <time dateTime={data.publishedAt} className="text-sm text-muted-foreground">
            {formatDate(data.publishedAt)}
          </time>
          {categoryLabel && (
            <span className="text-xs font-medium px-2 py-0.5 rounded-full bg-primary/10 text-primary">
              {categoryLabel}
            </span>
          )}
        </div>
        <h3 className="text-xl font-semibold mb-2">{data.title}</h3>
        <p className="text-foreground mb-4">{data.summary}</p>
      </div>
    </Link>
  );
}
