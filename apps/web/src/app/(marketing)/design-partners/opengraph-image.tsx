import { ImageResponse } from "next/og";

export const alt = "AllSource design partner program for durable AI agent memory";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function DesignPartnersOpenGraphImage() {
  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        background: "#0E1A2A",
        color: "white",
        padding: "64px 72px",
        position: "relative",
        overflow: "hidden",
        fontFamily: "Arial, sans-serif",
      }}
    >
      <div
        style={{
          position: "absolute",
          inset: 0,
          opacity: 0.17,
          backgroundImage:
            "linear-gradient(rgba(56,214,200,.35) 1px, transparent 1px), linear-gradient(90deg, rgba(47,140,255,.35) 1px, transparent 1px)",
          backgroundSize: "42px 42px",
        }}
      />
      <div style={{ display: "flex", flexDirection: "column", width: "840px", zIndex: 1 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            alignSelf: "flex-start",
            border: "1px solid rgba(56,214,200,.55)",
            borderRadius: 999,
            padding: "10px 18px",
            color: "#72EFE2",
            fontSize: 22,
            letterSpacing: 3,
          }}
        >
          FOUNDING COHORT · 5 TEAMS
        </div>
        <div
          style={{
            display: "flex",
            fontSize: 68,
            lineHeight: 1.02,
            fontWeight: 700,
            marginTop: 44,
          }}
        >
          Build agent memory that can explain itself.
        </div>
        <div
          style={{
            display: "flex",
            color: "#C5D0DF",
            fontSize: 28,
            lineHeight: 1.35,
            marginTop: 28,
          }}
        >
          60 hosted days · founder-led integration · durable provenance
        </div>
      </div>
      <div
        style={{
          position: "absolute",
          right: 72,
          top: 70,
          bottom: 70,
          width: 220,
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          borderLeft: "2px solid rgba(56,214,200,.55)",
          paddingLeft: 28,
          color: "#72EFE2",
          fontFamily: "monospace",
          fontSize: 18,
        }}
      >
        <span>application.opened</span>
        <span style={{ color: "#7AADEB" }}>memory.integrated</span>
        <span>recall.verified</span>
      </div>
      <div
        style={{
          position: "absolute",
          left: 72,
          bottom: 44,
          display: "flex",
          fontSize: 22,
          color: "#8FA3BC",
        }}
      >
        all-source.xyz/design-partners
      </div>
    </div>,
    size
  );
}
