import { ArrowDownRight, ArrowUpRight, GitBranch, Terminal } from "lucide-react";
import Link from "next/link";
import styles from "./harness-home.module.css";

const primeGuide = "https://github.com/all-source-os/all-source/tree/main/apps/prime-mcp";
const exampleEvidence = "/examples/checkout-graph.json";

const registerCommand =
  'codex mcp add allsource-example -- allsource-prime --mode mcp --data-dir "$PWD/.allsource-example"';

function Starter() {
  return (
    <section id="build-your-first-harness" className={styles.starter} aria-labelledby="setup-title">
      <div className={styles.sectionIntro}>
        <p className={styles.kicker}>
          <Terminal size={15} /> Self-service setup
        </p>
        <h2 id="setup-title">Build your first harness.</h2>
        <p>
          Start with one service and one question people keep asking. You’ll need Rust, a new
          scratch directory and a signed-in coding agent.
        </p>
        <a href={primeGuide} className={styles.textLink}>
          Prime installation guide <ArrowUpRight size={15} />
        </a>
      </div>
      <ol className={styles.steps}>
        <li>
          <h3>Connect your coding agent</h3>
          <p>
            Install Prime, then register it from the scratch directory. Codex stores the
            registration in your profile.
          </p>
          <pre>
            <code>{`cargo install allsource-prime\n${registerCommand}`}</code>
          </pre>
          <details>
            <summary>Connect Claude Code instead</summary>
            <p>
              Install Prime as above. Use this registration in the scratch project in place of the
              Codex command.
            </p>
            <pre>
              <code>
                {
                  'claude mcp add --scope local allsource-example -- allsource-prime --mode mcp --data-dir "$PWD/.allsource-example"'
                }
              </code>
            </pre>
            <p>Use one client at a time with this local store.</p>
          </details>
        </li>
        <li>
          <h3>Give it a decision to remember</h3>
          <p>
            Open your coding agent in this project and send the prompt below. If it asks for
            permission, approve the example’s graph writes. Close the session after it saves them.
          </p>
          <blockquote>
            Use allsource-example to save a service named Checkout and a team named Payments. Save
            the decision “Queue checkout requests during payment provider outages” with source
            ADR-17. Link Checkout to Payments with owned_by and to the decision with governed_by.
          </blockquote>
        </li>
        <li>
          <h3>Check what a fresh session knows</h3>
          <p>Reopen the same project with the same client and ask:</p>
          <blockquote>
            Use allsource-example to find Checkout. Who owns it, why does it use a queue, and what
            source is on the decision? Show the saved evidence.
          </blockquote>
          <p>
            Check for Payments and ADR-17. Then try one of your own services, with a source your
            team can inspect.
          </p>
          <details>
            <summary>Remove the example registration</summary>
            <p>Codex</p>
            <pre>
              <code>codex mcp remove allsource-example</code>
            </pre>
            <p>Claude Code</p>
            <pre>
              <code>claude mcp remove --scope local allsource-example</code>
            </pre>
          </details>
        </li>
      </ol>
    </section>
  );
}

export default function HarnessHome() {
  return (
    <>
      <section id="checkout-example" className={styles.example} aria-labelledby="example-title">
        <div className={styles.sectionIntro}>
          <p className={styles.kicker}>
            <GitBranch size={15} /> Worked example
          </p>
          <h2 id="example-title">Who owns checkout, and why does it use a queue?</h2>
        </div>
        <div className={styles.answer}>
          <p>
            Payments owns checkout. The team chose a queue so requests can wait during payment
            provider outages.
          </p>
          <dl>
            <div>
              <dt>Service owner</dt>
              <dd>Payments</dd>
            </div>
            <div>
              <dt>Decision record</dt>
              <dd>ADR-17</dd>
            </div>
          </dl>
          <a className={styles.textLink} href={exampleEvidence}>
            Inspect the saved evidence <ArrowUpRight size={15} />
          </a>
          <small>A fresh local Prime process retrieved both links from example company data.</small>
        </div>
      </section>
      <Starter />
      <section className={styles.workflows} aria-labelledby="workflows-title">
        <div className={styles.sectionIntro}>
          <p className={styles.kicker}>
            <ArrowDownRight size={16} /> More workflows
          </p>
          <h2 id="workflows-title">What else does your team keep looking up?</h2>
        </div>
        <div className={styles.workflowLinks}>
          <Link href="/solutions/agent-memory">
            <span>Engineering</span>
            <strong>Find the decision behind a service.</strong>
            <ArrowUpRight />
          </Link>
          <a href="/examples/support-approval.json">
            <span>Support</span>
            <strong>Trace a replacement approval to its case.</strong>
            <ArrowUpRight />
          </a>
          <div>
            <span>Go-to-market · example workflow</span>
            <strong>Trace an account decision to its owner and meeting notes.</strong>
          </div>
        </div>
      </section>
      <nav aria-label="Technical resources" className={styles.resources}>
        <Link href="/what-is-an-event-store">What is an event store?</Link>
        <Link href="/event-sourcing/patterns">Event sourcing patterns</Link>
        <Link href="/docs">Developer docs</Link>
        <Link href="/blog">Engineering notes</Link>
      </nav>
      <aside id="pricing" className={styles.help}>
        <div>
          <h2>Building something more specific?</h2>
          <p>
            Your team can start locally or explore hosted plans. Hands-on help is available for a
            custom build.
          </p>
        </div>
        <div className={styles.helpLinks}>
          <Link href="/pricing">
            See pricing <ArrowUpRight size={15} />
          </Link>
          <a href="mailto:sales@wolventech.com?subject=Custom%20engineering%20harness">
            Talk through a custom build <ArrowUpRight size={15} />
          </a>
        </div>
      </aside>
    </>
  );
}
