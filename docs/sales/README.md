# AllSource Sales & Marketing Artifacts

This folder contains all materials needed for SaaS sales pitches, demos, and marketing.

## Directory Structure

```
docs/sales/
├── README.md                           # This file
├── evaluation/
│   └── SAAS_EVALUATION.md             # SaaS readiness assessment
├── diagrams/
│   ├── C4_ARCHITECTURE_MERMAID.md     # C4 diagrams in Mermaid format
│   └── allsource.dsl                  # C4 diagrams in Structurizr DSL
├── pitch-deck/
│   ├── SALES_PITCH_DECK.md            # 14-slide pitch deck outline
│   └── VISUAL_ASSETS.md               # SVG graphics, charts, brand assets
├── videos/
│   └── VIDEO_SCRIPTS.md               # Scripts for 4 video types
├── SUMMARY.md
└── voice-file-comparison.md
```

## Quick Start Guide

### For Sales Presentations

1. **Start with**: `pitch-deck/SALES_PITCH_DECK.md`
   - 14-slide deck with speaker notes
   - Common objections & responses
   - 15-20 minute presentation

2. **Add visuals from**: `pitch-deck/VISUAL_ASSETS.md`
   - Performance comparison charts (SVG)
   - Architecture overview diagram
   - Feature highlight cards
   - Tech stack visualization

### For Technical Demos

1. **Follow scripts in**: `videos/VIDEO_SCRIPTS.md`
   - Product Overview (2-3 min) - landing page/social
   - Technical Demo (5-7 min) - developer evaluation
   - MCP Deep Dive (3-4 min) - AI/ML engineers
   - Architecture Walkthrough (4-5 min) - architects

2. **Use CLI recording setup** for terminal demos:
   ```bash
   brew install asciinema
   asciinema rec demo.cast
   ```

### For Architecture Discussions

1. **Mermaid format**: `diagrams/C4_ARCHITECTURE_MERMAID.md`
   - Renders in GitHub, Notion, VS Code
   - Context, Container, and 4 Component diagrams (Core, Control Plane, Query Service, MCP Server)
   - Data flow and deployment diagrams

2. **Structurizr DSL**: `diagrams/allsource.dsl`
   - Full C4 model with relationships
   - Export to PlantUML, PNG, SVG
   - Use with Structurizr Lite or CLI

### For Strategic Planning

1. **Review**: `evaluation/SAAS_EVALUATION.md`
   - Market positioning analysis
   - Competitive landscape
   - Pricing model recommendations
   - Go-to-market strategy
   - Risk assessment
   - Financial projections

## Key Value Propositions

Use these consistently across all materials:

| Message | Supporting Data |
|---------|----------------|
| **AI-Native First** | Native MCP server (73 tool definitions) |
| **Blazing Performance** | 469K events/sec, 11.9μs p99 latency |
| **Polyglot Excellence** | Rust + Go + Elixir = best tool for each job |
| **Self-Host** | Apache-2.0 community edition, `docker compose up` |
| **Enterprise Security** | Multi-tenancy, RBAC, audit logging |
| **Time-Travel** | Query any point in history instantly |

## Brand Guidelines

### Colors
```
Primary:    #ce422b  (Rust/Fire)
Secondary:  #00ADD8  (Go Cyan)
Tertiary:   #4E2A8E  (Elixir Purple)
Accent:     #4ecdc4  (Teal/AI)
Dark BG:    #0a0e27  (Deep Navy)
Light Text: #f8fafc  (Off-white)
```

### Tone
- Technical but accessible
- Confident, not arrogant
- Data-driven claims
- Developer-focused

## Generating Assets

### Export Mermaid to PNG
```bash
npm install -g @mermaid-js/mermaid-cli
mmdc -i diagram.mmd -o diagram.png -t dark
```

### Export Structurizr to PlantUML
```bash
# Download Structurizr CLI
# https://github.com/structurizr/cli
structurizr-cli export -workspace allsource.dsl -format plantuml
```

### Record Terminal Demos
```bash
brew install asciinema
asciinema rec demo.cast --title "AllSource Demo"
# Convert to GIF
agg demo.cast demo.gif --theme monokai
```

## Checklist: Sales Pitch Preparation

- [ ] Review prospect's industry and tech stack
- [ ] Customize demo to their use case
- [ ] Test all CLI commands locally
- [ ] Prepare backup video in case of issues
- [ ] Print/export key slides
- [ ] Prepare POC offer
- [ ] Have pricing flexibility approved
- [ ] Schedule follow-up before meeting

## Updates

These materials should be updated when:
- Performance benchmarks change
- New features are released
- Pricing model changes
- New customer testimonials available
- Competitive landscape shifts

Last updated: 2026-11-23
Version: Core 0.30.0
