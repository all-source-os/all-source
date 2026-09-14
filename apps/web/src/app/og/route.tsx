import { ImageResponse } from "next/og";
import type { NextRequest } from "next/server";
import { siteConfig } from "@/lib/config";

export const runtime = "edge";

export async function GET(req: NextRequest) {
  const { searchParams } = req.nextUrl;
  const postTitle = searchParams.get("title") || siteConfig.description;
  const font = fetch(new URL("../../assets/fonts/Inter-SemiBold.ttf", import.meta.url)).then(
    (res) => res.arrayBuffer()
  );
  const fontData = await font;

  return new ImageResponse(
    <div
      style={{
        height: "100%",
        width: "100%",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        // Branded dark gradient. Replaces a former url(/og.png) reference whose
        // asset was missing, leaving every generated card on blank white.
        backgroundColor: "#000",
        backgroundImage:
          "radial-gradient(circle at 25% 15%, #1e293b 0%, transparent 45%), radial-gradient(circle at 80% 0%, #0f172a 0%, transparent 50%)",
        color: "#fff",
        fontSize: 32,
        fontWeight: 600,
      }}
    >
      <div
        style={{
          position: "relative",
          display: "flex",
          flexDirection: "column",
          justifyContent: "center",
          alignItems: "center",
          top: "125px",
        }}
      >
        {/* Inline SVG logo — avoids importing @allsource/ui which bloats the edge bundle past 1MB */}
        <svg
          width="64"
          height="64"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          role="img"
          aria-label="AllSource logo"
        >
          <circle cx="12" cy="12" r="10" />
          <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" />
          <path d="M2 12h20" />
        </svg>

        <div
          style={{
            display: "flex",
            justifyContent: "center",
            alignItems: "center",
            fontSize: "64px",
            fontWeight: "600",
            marginTop: "24px",
            textAlign: "center",
            width: "80%",
            letterSpacing: "-0.05em",
          }}
        >
          {postTitle}
        </div>
        <div
          style={{
            display: "flex",
            fontSize: "16px",
            fontWeight: "500",
            marginTop: "16px",
            color: "#808080",
          }}
        >
          {siteConfig.name}
        </div>
      </div>

      <img
        src={`${siteConfig.url}/dashboard.png`}
        alt="Dashboard preview"
        width={900}
        height={539}
        style={{
          position: "relative",
          bottom: -160,
          border: "4px solid lightgray",
          background: "lightgray",
          borderRadius: 20,
          zIndex: 1,
        }}
      />
    </div>,
    {
      width: 1200,
      height: 630,
      headers: {
        "Cache-Control": "public, max-age=3600, s-maxage=86400, stale-while-revalidate=604800",
      },
      fonts: [
        {
          name: "Inter",
          data: fontData,
          style: "normal",
        },
      ],
    }
  );
}
