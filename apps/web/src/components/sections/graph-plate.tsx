"use client";

import { ArrowDownRight, GitBranch } from "lucide-react";
import Image from "next/image";
import { useState } from "react";
import styles from "./harness-home.module.css";

const records = [
  {
    name: "Checkout",
    kind: "Service",
    text: "Two saved links connect Checkout to its owner and design decision.",
    position: styles.checkout,
  },
  {
    name: "Payments",
    kind: "Owner",
    text: "The Payments team owns Checkout.",
    position: styles.payments,
  },
  {
    name: "ADR-17",
    kind: "Source",
    text: "Queue checkout requests during payment provider outages. Source recorded on the decision: ADR-17.",
    position: styles.source,
  },
] as const;

export default function GraphPlate() {
  const [record, setRecord] = useState<(typeof records)[number]>(records[0]);

  return (
    <figure className={styles.plate} aria-label="Explore the example service graph">
      <div className={styles.plateHead}>
        <span>
          <GitBranch size={16} /> Service graph
        </span>
        <span>Local example</span>
      </div>
      <div className={styles.graphStage}>
        <Image
          src="/assets/prime-memory-sculpture.webp"
          alt=""
          width={1536}
          height={1024}
          sizes="(max-width: 760px) 94vw, 52vw"
          preload
        />
        <fieldset aria-label="Inspect a graph record">
          {records.map((item) => (
            <button
              key={item.name}
              type="button"
              className={`${styles.node} ${item.position}`}
              onClick={() => setRecord(item)}
              aria-pressed={record === item}
            >
              <span aria-hidden="true" />
              {item.name}
            </button>
          ))}
        </fieldset>
      </div>
      <figcaption className={styles.readout} aria-live="polite" aria-atomic="true">
        <ArrowDownRight aria-hidden="true" size={19} />
        <div>
          <b>
            {record.kind} / {record.name}
          </b>
          <p>{record.text}</p>
        </div>
      </figcaption>
    </figure>
  );
}
