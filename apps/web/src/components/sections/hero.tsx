import { ArrowDown, ArrowUpRight } from "lucide-react";
import GraphPlate from "./graph-plate";
import styles from "./harness-home.module.css";

export default function Hero() {
  return (
    <section id="hero" className={styles.hero} aria-labelledby="harness-title">
      <div className={styles.heroCopy}>
        <p className={styles.kicker}>
          <span className={styles.trace} aria-hidden="true" />
          AllSource Prime · Agent memory
        </p>
        <h1 id="harness-title">
          Build an AI agent that <em>knows your systems.</em>
        </h1>
        <p className={styles.lead}>
          Give your coding agent a memory of how your company works. Connect services to owners and
          decisions, and find the source in a fresh session.
        </p>
        <div className={styles.actions}>
          <a className={styles.primary} href="#build-your-first-harness">
            Build your first harness <ArrowUpRight size={18} />
          </a>
          <a className={styles.textLink} href="#checkout-example">
            See an example <ArrowDown size={16} />
          </a>
        </div>
        <p className={styles.clientLine}>
          For engineering teams using <strong>Codex</strong> or <strong>Claude Code</strong>.
        </p>
      </div>
      <GraphPlate />
    </section>
  );
}
