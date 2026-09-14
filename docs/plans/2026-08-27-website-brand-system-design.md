# Website brand-system migration

## Audit

Canonical product-identity decision requires existing background, foreground,
border, and primary tokens with no new decorative palette. Logo supplies a
coherent blue family. Website currently defaults to a near-black global theme;
several marketing routes add one-off near-black fields, purple decoration,
neutral terminal panels, and direct hex values. Marketing assets also use an
older indigo palette unrelated to current logo.

## Chosen approach

Add a marketing-scoped semantic theme derived from the logo and approved deep
`#063A6C` field. Use `#07549A` for raised blue surfaces. Home and every route
under marketing layout inherit it. Keep dashboard on existing product UI
tokens.

Rejected approaches:

- Rewriting root tokens would recolor dashboard and authentication UI.
- Replacing colors page by page would preserve no enforceable source of truth.
- Locking one theme would make existing theme control ineffective.

## System

Light marketing theme uses paper, source ink, and core blue. Default dark
marketing theme uses deep field, mid-blue cards, paper text, and ice-blue
actions. Code and diagnostic panels use source ink. Status colors stay
semantic.

Typography remains system sans plus functional monospace. Provenance rail is
the only reusable signature and must describe real event flow.

## Implementation

- Add named brand colors and marketing semantic overrides in `globals.css`.
- Scope homepage and marketing layout with `marketing-theme`.
- Align browser theme colors, campaign page, social image, terminal surfaces,
  decorative accents, navigation, footer, and theme control.
- Remove stale Instagram identity from structured data and configuration.
- Keep component APIs and page information architecture unchanged.

## Acceptance

- Home, product map, pricing, design partners, platform, solution, and docs
  routes visibly share blue brand system.
- Dashboard remains on existing product theme.
- No near-black full-page marketing field or decorative purple remains.
- Code panels remain visually distinct and branded as source-ink surfaces.
- Light and dark marketing themes both meet WCAG 2.2 AA.
- No horizontal overflow at 320px; no console or server errors.
