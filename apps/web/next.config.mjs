/** @type {import('next').NextConfig} */
const nextConfig = {
  // All production frontends run as Fly.io containers.
  output: "standalone",
  // Transpile monorepo packages and icon libraries for proper SSR bundling
  transpilePackages: ["@allsource/ui", "react-icons"],
  images: {
    remotePatterns: [{ hostname: "localhost" }, { hostname: "randomuser.me" }],
  },
  // OAuth proxy moved from rewrites (build-time) to a runtime API route at
  // src/app/api/v1/auth/oauth/[...path]/route.ts so CONTROL_PLANE_INTERNAL_URL
  // is read at request time, not baked in during the Vercel build.

  // Security headers (fixes #123)
  async headers() {
    const headers = [
      {
        source: "/(.*)",
        headers: [
          {
            key: "Content-Security-Policy",
            value: [
              "default-src 'self'",
              `script-src 'self' 'unsafe-inline'${process.env.NODE_ENV === "production" ? "" : " 'unsafe-eval'"} https://www.googletagmanager.com https://eu-assets.i.posthog.com https://challenges.cloudflare.com`,
              "style-src 'self' 'unsafe-inline' https://rsms.me",
              "img-src 'self' data: blob: https://randomuser.me",
              "font-src 'self' data: https://rsms.me",
              "media-src 'self'",
              "connect-src 'self' ws: wss: https://api.all-source.xyz https://allsource-query.fly.dev https://www.google-analytics.com https://*.google-analytics.com https://analytics.google.com https://eu.i.posthog.com https://eu-assets.i.posthog.com https://challenges.cloudflare.com",
              "frame-src 'self' https://challenges.cloudflare.com",
              "frame-ancestors 'none'",
              "base-uri 'self'",
              "form-action 'self'",
            ].join("; "),
          },
          {
            key: "Strict-Transport-Security",
            value: "max-age=63072000; includeSubDomains; preload",
          },
          {
            key: "X-Frame-Options",
            value: "DENY",
          },
          {
            key: "X-Content-Type-Options",
            value: "nosniff",
          },
          {
            key: "Referrer-Policy",
            value: "strict-origin-when-cross-origin",
          },
          {
            key: "Permissions-Policy",
            value: "camera=(), microphone=(), geolocation=()",
          },
        ],
      },
    ];
    const policy = headers[0].headers.find((header) => header.key === "Content-Security-Policy");
    return [
      ...headers,
      ...["/api/customer-agent/:path*", "/mcp/customer-review", "/.well-known/:path*"].map(
        (source) => ({
          source,
          headers: [{ key: "Referrer-Policy", value: "no-referrer" }],
        })
      ),
      {
        source: "/connect/claude",
        headers: [
          // Browsers apply form-action to the consent POST's final OAuth redirect.
          {
            key: "Content-Security-Policy",
            value: policy.value.replace(
              "form-action 'self'",
              "form-action 'self' https://claude.ai/api/mcp/auth_callback"
            ),
          },
          { key: "Referrer-Policy", value: "no-referrer" },
          { key: "X-Robots-Tag", value: "noindex, nofollow" },
        ],
      },
    ];
  },
};

export default nextConfig;
