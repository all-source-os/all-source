# AllSource Visual Assets for Sales

## Brand Colors

```
Primary:    #ce422b (Rust/Fire - represents Rust core)
Secondary:  #00ADD8 (Go Cyan - represents Control Plane)
Tertiary:   #4E2A8E (Elixir Purple - represents Query/MCP)
Accent:     #4ecdc4 (Teal - represents AI/Innovation)
Dark:       #0a0e27 (Deep Navy - backgrounds)
Light:      #f8fafc (Off-white - text on dark)
```

---

## 1. Performance Metrics Visualization

### Ingestion Throughput Chart (SVG)

```svg
<svg viewBox="0 0 600 400" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="allsourceGrad" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" style="stop-color:#ce422b"/>
      <stop offset="100%" style="stop-color:#8b1a1a"/>
    </linearGradient>
  </defs>

  <!-- Background -->
  <rect width="600" height="400" fill="#0a0e27"/>

  <!-- Title -->
  <text x="300" y="35" text-anchor="middle" fill="#f8fafc" font-family="system-ui" font-size="20" font-weight="bold">
    AllSource ingestion throughput (events/sec)
  </text>

  <!-- Y-axis labels -->
  <text x="45" y="80" text-anchor="end" fill="#888" font-family="system-ui" font-size="12">500K</text>
  <text x="45" y="140" text-anchor="end" fill="#888" font-family="system-ui" font-size="12">400K</text>
  <text x="45" y="200" text-anchor="end" fill="#888" font-family="system-ui" font-size="12">300K</text>
  <text x="45" y="260" text-anchor="end" fill="#888" font-family="system-ui" font-size="12">200K</text>
  <text x="45" y="320" text-anchor="end" fill="#888" font-family="system-ui" font-size="12">100K</text>

  <!-- Grid lines -->
  <line x1="55" y1="75" x2="580" y2="75" stroke="#333" stroke-width="1"/>
  <line x1="55" y1="135" x2="580" y2="135" stroke="#333" stroke-width="1"/>
  <line x1="55" y1="195" x2="580" y2="195" stroke="#333" stroke-width="1"/>
  <line x1="55" y1="255" x2="580" y2="255" stroke="#333" stroke-width="1"/>
  <line x1="55" y1="315" x2="580" y2="315" stroke="#333" stroke-width="1"/>
  <line x1="55" y1="350" x2="580" y2="350" stroke="#666" stroke-width="2"/>

  <!-- Bars -->
  <!-- AllSource: 469K -->
  <rect x="80" y="77" width="80" height="273" fill="url(#allsourceGrad)" rx="4"/>
  <text x="120" y="68" text-anchor="middle" fill="#4ecdc4" font-family="system-ui" font-size="14" font-weight="bold">469K</text>

  <!-- X-axis labels -->
  <text x="120" y="375" text-anchor="middle" fill="#f8fafc" font-family="system-ui" font-size="12" font-weight="bold">AllSource</text>
</svg>
```

### Query Latency Chart (SVG)

```svg
<svg viewBox="0 0 600 350" xmlns="http://www.w3.org/2000/svg">
  <rect width="600" height="350" fill="#0a0e27"/>

  <text x="300" y="35" text-anchor="middle" fill="#f8fafc" font-family="system-ui" font-size="20" font-weight="bold">
    Query Latency p99 (microseconds)
  </text>

  <!-- AllSource: 11.9us p99 -->
  <rect x="80" y="80" width="100" height="40" fill="#4ecdc4" rx="4"/>
  <text x="200" y="105" fill="#f8fafc" font-family="system-ui" font-size="14" font-weight="bold">11.9 us - AllSource</text>

  <!-- Legend -->
  <text x="300" y="330" text-anchor="middle" fill="#4ecdc4" font-family="system-ui" font-size="16">
    AllSource p99 query latency: 11.9 us
  </text>
</svg>
```

---

## 2. Architecture Overview (Simplified Sales Version)

```svg
<svg viewBox="0 0 800 500" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
      <feDropShadow dx="2" dy="2" stdDeviation="3" flood-opacity="0.3"/>
    </filter>
  </defs>

  <rect width="800" height="500" fill="#0a0e27"/>

  <!-- Title -->
  <text x="400" y="40" text-anchor="middle" fill="#f8fafc" font-family="system-ui" font-size="24" font-weight="bold">
    AllSource Architecture
  </text>
  <text x="400" y="65" text-anchor="middle" fill="#888" font-family="system-ui" font-size="14">
    Polyglot Design: Best Language for Each Job
  </text>

  <!-- Users/Clients Layer -->
  <rect x="50" y="90" width="700" height="60" fill="#1a1f36" rx="8" filter="url(#shadow)"/>
  <text x="400" y="125" text-anchor="middle" fill="#f8fafc" font-family="system-ui" font-size="14">
    Web Dashboard | REST API | WebSocket | AI Agents (MCP)
  </text>

  <!-- Arrow down -->
  <path d="M400 150 L400 170 L390 160 M400 170 L410 160" stroke="#4ecdc4" stroke-width="2" fill="none"/>

  <!-- Control Plane (Go) -->
  <rect x="200" y="180" width="400" height="70" fill="#00ADD8" rx="8" filter="url(#shadow)"/>
  <text x="400" y="210" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="16" font-weight="bold">
    Control Plane (Go)
  </text>
  <text x="400" y="235" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="12">
    Auth | RBAC | Audit | Routing | OpenTelemetry
  </text>

  <!-- Arrows down -->
  <path d="M300 250 L300 280" stroke="#4ecdc4" stroke-width="2"/>
  <path d="M500 250 L500 280" stroke="#4ecdc4" stroke-width="2"/>

  <!-- Core Services Row -->
  <!-- Rust Core -->
  <rect x="50" y="290" width="220" height="100" fill="#ce422b" rx="8" filter="url(#shadow)"/>
  <text x="160" y="325" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="16" font-weight="bold">
    Event Store (Rust)
  </text>
  <text x="160" y="350" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">469K events/sec</text>
  <text x="160" y="370" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">11.9us latency</text>

  <!-- Query Service -->
  <rect x="290" y="290" width="220" height="100" fill="#4E2A8E" rx="8" filter="url(#shadow)"/>
  <text x="400" y="325" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="16" font-weight="bold">
    Query Service (Elixir)
  </text>
  <text x="400" y="350" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">Real-time Projections</text>
  <text x="400" y="370" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">WebSocket Streaming</text>

  <!-- MCP Server -->
  <rect x="530" y="290" width="220" height="100" fill="#4E2A8E" rx="8" filter="url(#shadow)"/>
  <text x="640" y="325" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="16" font-weight="bold">
    MCP Server (Elixir)
  </text>
  <text x="640" y="350" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">73 AI-Native Tools</text>
  <text x="640" y="370" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">Natural Language Queries</text>

  <!-- Storage Layer -->
  <rect x="50" y="420" width="700" height="60" fill="#1a1f36" rx="8" filter="url(#shadow)"/>
  <text x="400" y="455" text-anchor="middle" fill="#f8fafc" font-family="system-ui" font-size="14">
    Parquet Storage | Write-Ahead Log | DashMap
  </text>

  <!-- Arrows to storage -->
  <path d="M160 390 L160 420" stroke="#888" stroke-width="1"/>
  <path d="M400 390 L400 420" stroke="#888" stroke-width="1"/>
</svg>
```

---

## 3. Feature Highlight Cards

### AI-Native Integration Card

```svg
<svg viewBox="0 0 400 250" xmlns="http://www.w3.org/2000/svg">
  <rect width="400" height="250" fill="#0a0e27" rx="12"/>
  <rect x="10" y="10" width="380" height="230" fill="#1a1f36" rx="8"/>

  <!-- Icon placeholder -->
  <circle cx="50" cy="50" r="25" fill="#4ecdc4"/>
  <text x="50" y="55" text-anchor="middle" fill="#0a0e27" font-family="system-ui" font-size="20">AI</text>

  <!-- Title -->
  <text x="90" y="45" fill="#f8fafc" font-family="system-ui" font-size="18" font-weight="bold">AI-Native First</text>
  <text x="90" y="65" fill="#4ecdc4" font-family="system-ui" font-size="12">Model Context Protocol</text>

  <!-- Features list -->
  <text x="30" y="100" fill="#f8fafc" font-family="system-ui" font-size="13">73 tool definitions for Claude/GPT agents</text>
  <text x="30" y="125" fill="#f8fafc" font-family="system-ui" font-size="13">Natural language event queries</text>
  <text x="30" y="150" fill="#f8fafc" font-family="system-ui" font-size="13">Pattern detection & analysis</text>
  <text x="30" y="175" fill="#f8fafc" font-family="system-ui" font-size="13">TOON format (about 50% fewer tokens, tabular data)</text>

  <!-- Stat -->
  <text x="200" y="220" text-anchor="middle" fill="#4ecdc4" font-family="system-ui" font-size="24" font-weight="bold">
    Only event store with native MCP
  </text>
</svg>
```

### Performance Card

```svg
<svg viewBox="0 0 400 250" xmlns="http://www.w3.org/2000/svg">
  <rect width="400" height="250" fill="#0a0e27" rx="12"/>
  <rect x="10" y="10" width="380" height="230" fill="#1a1f36" rx="8"/>

  <!-- Stats -->
  <text x="200" y="60" text-anchor="middle" fill="#ce422b" font-family="system-ui" font-size="48" font-weight="bold">469K</text>
  <text x="200" y="85" text-anchor="middle" fill="#888" font-family="system-ui" font-size="14">events per second</text>

  <line x1="50" y1="110" x2="350" y2="110" stroke="#333" stroke-width="1"/>

  <text x="120" y="150" text-anchor="middle" fill="#4ecdc4" font-family="system-ui" font-size="32" font-weight="bold">11.9us</text>
  <text x="120" y="175" text-anchor="middle" fill="#888" font-family="system-ui" font-size="12">p99 latency</text>

  <text x="280" y="150" text-anchor="middle" fill="#4ecdc4" font-family="system-ui" font-size="32" font-weight="bold">73</text>
  <text x="280" y="175" text-anchor="middle" fill="#888" font-family="system-ui" font-size="12">MCP tool definitions</text>

  <text x="200" y="220" text-anchor="middle" fill="#f8fafc" font-family="system-ui" font-size="14">
    Rust-powered. Lock-free indexing.
  </text>
</svg>
```

---

## 4. Tech Stack Visualization

```svg
<svg viewBox="0 0 700 300" xmlns="http://www.w3.org/2000/svg">
  <rect width="700" height="300" fill="#0a0e27"/>

  <text x="350" y="35" text-anchor="middle" fill="#f8fafc" font-family="system-ui" font-size="20" font-weight="bold">
    Polyglot Architecture: Best Tool for Each Job
  </text>

  <!-- Rust -->
  <rect x="30" y="70" width="150" height="180" fill="#ce422b" rx="8"/>
  <text x="105" y="100" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="16" font-weight="bold">Rust</text>
  <text x="105" y="125" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">Event Store Core</text>
  <line x1="50" y1="140" x2="160" y2="140" stroke="#fff" stroke-opacity="0.3"/>
  <text x="105" y="160" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Zero-cost abstractions</text>
  <text x="105" y="180" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Memory safety</text>
  <text x="105" y="200" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Fearless concurrency</text>
  <text x="105" y="230" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="12" font-weight="bold">Rust 1.92</text>

  <!-- Go -->
  <rect x="200" y="70" width="150" height="180" fill="#00ADD8" rx="8"/>
  <text x="275" y="100" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="16" font-weight="bold">Go</text>
  <text x="275" y="125" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">Control Plane</text>
  <line x1="220" y1="140" x2="330" y2="140" stroke="#fff" stroke-opacity="0.3"/>
  <text x="275" y="160" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Fast compilation</text>
  <text x="275" y="180" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Built-in concurrency</text>
  <text x="275" y="200" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Ops-friendly</text>
  <text x="275" y="230" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="12" font-weight="bold">Go 1.26.11</text>

  <!-- Elixir -->
  <rect x="370" y="70" width="150" height="180" fill="#4E2A8E" rx="8"/>
  <text x="445" y="100" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="16" font-weight="bold">Elixir</text>
  <text x="445" y="125" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">Query + MCP</text>
  <line x1="390" y1="140" x2="500" y2="140" stroke="#fff" stroke-opacity="0.3"/>
  <text x="445" y="160" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">BEAM reliability</text>
  <text x="445" y="180" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Real-time WebSocket</text>
  <text x="445" y="200" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Pattern matching</text>
  <text x="445" y="230" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="12" font-weight="bold">Elixir 1.17</text>

  <!-- TypeScript -->
  <rect x="540" y="70" width="150" height="180" fill="#3178c6" rx="8"/>
  <text x="615" y="100" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="16" font-weight="bold">TypeScript</text>
  <text x="615" y="125" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="11">Web Dashboard</text>
  <line x1="560" y1="140" x2="670" y2="140" stroke="#fff" stroke-opacity="0.3"/>
  <text x="615" y="160" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">React 19 + Next.js</text>
  <text x="615" y="180" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Type safety</text>
  <text x="615" y="200" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="10">Modern DX</text>
  <text x="615" y="230" text-anchor="middle" fill="#fff" font-family="system-ui" font-size="12" font-weight="bold">Next.js 16</text>

  <!-- Total -->
  <text x="350" y="280" text-anchor="middle" fill="#4ecdc4" font-family="system-ui" font-size="16" font-weight="bold">
    Runtimes: Rust 1.92, Go 1.26.11, Elixir 1.17, Next.js 16
  </text>
</svg>
```

---

## 5. Data for Charts (Python/Plotly)

```python
import plotly.graph_objects as go
import plotly.express as px

# Throughput comparison
throughput_data = {
    'System': ['AllSource'],
    'Events/sec': [469000],
    'Color': ['#ce422b']
}

fig = go.Figure(data=[
    go.Bar(x=throughput_data['System'],
           y=throughput_data['Events/sec'],
           marker_color=throughput_data['Color'])
])
fig.update_layout(
    title='Event Throughput Comparison',
    yaxis_title='Events per Second',
    template='plotly_dark'
)
fig.write_image('throughput_comparison.png', scale=2)

# Latency comparison
latency_data = {
    'System': ['AllSource'],
    'Latency_us': [11.9]
}

fig2 = go.Figure(data=[
    go.Bar(x=latency_data['System'],
           y=latency_data['Latency_us'],
           marker_color=['#4ecdc4'])
])
fig2.update_layout(
    title='Query Latency (p99) - Lower is Better',
    yaxis_title='Microseconds',
    yaxis_type='log',
    template='plotly_dark'
)
fig2.write_image('latency_comparison.png', scale=2)

```

---

## 6. Export Instructions

### For PNG/SVG Export

1. **Mermaid diagrams**: Use `mmdc` CLI
   ```bash
   npm install -g @mermaid-js/mermaid-cli
   mmdc -i diagram.mmd -o diagram.png -t dark -b transparent
   ```

2. **SVG files**: Save as `.svg` and convert with Inkscape or ImageMagick
   ```bash
   inkscape --export-type=png --export-dpi=300 diagram.svg
   ```

3. **Python charts**: Install dependencies
   ```bash
   pip install plotly kaleido pandas
   ```

### For Presentations

- Export at 2x resolution for retina displays
- Use dark backgrounds for consistency
- Maintain brand colors across all assets
