import { describe, expect, it } from "vitest";
import { getBlogPosts, getPost } from "@/lib/blog";

describe("blog metadata listing", () => {
  it("lists card fields without compiling every article", async () => {
    const posts = await getBlogPosts();

    expect(posts.length).toBeGreaterThan(0);
    expect(posts.every((post) => !("source" in post))).toBe(true);
    expect(posts.every((post) => post.title && post.slug && post.publishedAt && post.image)).toBe(
      true
    );

    const firstPost = posts[0];
    if (!firstPost) throw new Error("Expected at least one blog post");
    const detail = await getPost(firstPost.slug);
    expect(detail?.source).toContain("<");
  });
});
