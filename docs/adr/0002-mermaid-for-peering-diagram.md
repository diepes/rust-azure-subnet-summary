# Mermaid for peering diagram output

The peering diagram shows VNet Peering Edges as connections between VNets, grouped into Subscription Island subgraphs, with Gateway VNets annotated with an external-connectivity node.

We chose **Mermaid** (embedded in a `.md` file) as the output format.

## Considered Options

- **Mermaid (.md)** — renders natively in GitHub pull requests, Confluence pages, and most modern markdown viewers with no extra tooling. Chosen option.
- **DOT/Graphviz (.dot)** — superior layout algorithms and more expressive, but requires Graphviz installed locally or a separate render step; does not render in GitHub.
- **ASCII art** — zero dependencies, but unreadable for diagrams with more than ~10 nodes and not exportable.

## Decision

Mermaid wins on accessibility: the output file can be dropped into a GitHub PR or Confluence page and renders immediately. If richer layout is needed later, the Mermaid generator can be replaced without changing the data pipeline.
