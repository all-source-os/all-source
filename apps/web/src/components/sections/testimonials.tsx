"use client";

import { cn, Marquee, Section } from "@allsource/ui";
import { Star } from "lucide-react";
import { motion } from "motion/react";
import Image from "next/image";

export const Highlight = ({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) => {
  return (
    <span
      className={cn(
        "bg-primary/20 p-1 py-0.5 font-bold text-primary dark:bg-primary/20 dark:text-primary",
        className
      )}
    >
      {children}
    </span>
  );
};

export interface TestimonialCardProps {
  name: string;
  role: string;
  img?: string;
  description: React.ReactNode;
  className?: string;
  [key: string]: unknown;
}

export const TestimonialCard = ({
  description,
  name,
  img,
  role,
  className,
  ...props
}: TestimonialCardProps) => (
  <div
    className={cn(
      "mb-4 flex w-full cursor-pointer break-inside-avoid flex-col items-center justify-between gap-6 rounded-xl p-4",
      " border border-neutral-200 bg-white",
      "dark:bg-black dark:[border:1px_solid_rgba(255,255,255,.1)] dark:[box-shadow:0_-20px_80px_-20px_#ffffff1f_inset]",
      className
    )}
    {...props}
  >
    <div className="select-none text-sm font-normal text-neutral-700 dark:text-neutral-400">
      {description}
      <div className="flex flex-row py-1">
        <Star className="size-4 text-yellow-500 fill-yellow-500" />
        <Star className="size-4 text-yellow-500 fill-yellow-500" />
        <Star className="size-4 text-yellow-500 fill-yellow-500" />
        <Star className="size-4 text-yellow-500 fill-yellow-500" />
        <Star className="size-4 text-yellow-500 fill-yellow-500" />
      </div>
    </div>

    <div className="flex w-full select-none items-center justify-start gap-5">
      <Image
        width={40}
        height={40}
        src={img || ""}
        alt={name}
        className="h-10 w-10 rounded-full ring-1 ring-border ring-offset-4"
      />

      <div>
        <p className="font-medium text-neutral-500">{name}</p>
        <p className="text-xs font-normal text-neutral-400">{role}</p>
      </div>
    </div>
  </div>
);

const testimonials = [
  {
    name: "Marcus Chen",
    role: "CTO at FinScale",
    img: "https://randomuser.me/api/portraits/men/91.jpg",
    description: (
      <p>
        We migrated from Kafka + PostgreSQL to AllSource for our trading audit logs.
        <Highlight>Query latency dropped from 200ms to under 12 microseconds.</Highlight> The
        time-travel queries saved our compliance team hundreds of hours.
      </p>
    ),
  },
  {
    name: "Sarah Mitchell",
    role: "Head of Engineering at DataFlow",
    img: "https://randomuser.me/api/portraits/women/12.jpg",
    description: (
      <p>
        The MCP Server integration is a game-changer.
        <Highlight>Our AI agents now autonomously detect anomalies and trigger alerts.</Highlight>{" "}
        What used to require a dedicated team now runs itself.
      </p>
    ),
  },
  {
    name: "Raj Krishnamurthy",
    role: "Founder at IoTStream",
    img: "https://randomuser.me/api/portraits/men/45.jpg",
    description: (
      <p>
        Processing 2M sensor events per minute was crushing our infrastructure.
        <Highlight>AllSource handles it with a 129MB footprint.</Highlight> The Rust core is
        incredibly efficient.
      </p>
    ),
  },
  {
    name: "Elena Rodriguez",
    role: "Platform Lead at CloudNative Inc",
    img: "https://randomuser.me/api/portraits/women/83.jpg",
    description: (
      <p>
        Event sourcing used to mean complexity. AllSource changed that.
        <Highlight>We went from idea to production in two weeks.</Highlight> The stream processing
        pipelines are intuitive and powerful.
      </p>
    ),
  },
  {
    name: "David Park",
    role: "Principal Architect at TechVentures",
    img: "https://randomuser.me/api/portraits/men/1.jpg",
    description: (
      <p>
        The polyglot architecture (Rust, Go, Elixir) worried me at first.
        <Highlight>
          Turns out using the right tool for each job actually simplifies operations.
        </Highlight>{" "}
        Each service does one thing exceptionally well.
      </p>
    ),
  },
  {
    name: "Lisa Wang",
    role: "VP of Data at RetailGiant",
    img: "https://randomuser.me/api/portraits/women/5.jpg",
    description: (
      <p>
        Our inventory system needed complete audit trails for SOX compliance.
        <Highlight>
          AllSource gave us immutable event logs with zero schema migration headaches.
        </Highlight>{" "}
        Auditors love the time-travel queries.
      </p>
    ),
  },
  {
    name: "James Morrison",
    role: "Engineering Manager at HealthTech",
    img: "https://randomuser.me/api/portraits/men/14.jpg",
    description: (
      <p>
        Patient data requires bulletproof security. AllSource&apos;s RBAC and multi-tenancy
        <Highlight>passed our security audit with flying colors.</Highlight> The audit logging is
        comprehensive.
      </p>
    ),
  },
  {
    name: "Priya Sharma",
    role: "Staff Engineer at GameStudio",
    img: "https://randomuser.me/api/portraits/women/56.jpg",
    description: (
      <p>
        Game telemetry generates massive event volumes.
        <Highlight>469K events/sec ingestion means we never drop player data.</Highlight> The
        analytics team can query any moment in a player&apos;s journey.
      </p>
    ),
  },
  {
    name: "Michael Torres",
    role: "DevOps Lead at ScaleUp",
    img: "https://randomuser.me/api/portraits/men/18.jpg",
    description: (
      <p>
        Deploying was dead simple - Docker, Helm charts, K8s manifests all provided.
        <Highlight>Total footprint under 130MB for the entire stack.</Highlight> Resource efficiency
        we&apos;ve never seen before.
      </p>
    ),
  },
  {
    name: "Amanda Foster",
    role: "Data Science Lead at AIFirst",
    img: "https://randomuser.me/api/portraits/women/73.jpg",
    description: (
      <p>
        The 73 MCP tools transformed how we work with event data.
        <Highlight>
          Claude can now query, analyze, and even manage our event streams directly.
        </Highlight>{" "}
        True AI-native infrastructure.
      </p>
    ),
  },
  {
    name: "Kevin O&apos;Brien",
    role: "CTO at LogisticsAI",
    img: "https://randomuser.me/api/portraits/men/25.jpg",
    description: (
      <p>
        Supply chain events are complex and time-sensitive.
        <Highlight>
          AllSource&apos;s stream processing pipelines handle our branching logic elegantly.
        </Highlight>{" "}
        Filter, enrich, route - all in real-time.
      </p>
    ),
  },
  {
    name: "Nina Patel",
    role: "Engineering Director at SaaSCo",
    img: "https://randomuser.me/api/portraits/women/78.jpg",
    description: (
      <p>
        Multi-tenancy was a hard requirement. AllSource&apos;s built-in isolation
        <Highlight>
          means each customer&apos;s data is completely separated at the infrastructure level.
        </Highlight>{" "}
        No custom code needed.
      </p>
    ),
  },
  {
    name: "Alex Thompson",
    role: "Open Source Maintainer",
    img: "https://randomuser.me/api/portraits/men/54.jpg",
    description: (
      <p>
        Finally, an event store that&apos;s truly open source with Apache-2.0 licensing.
        <Highlight>The codebase is clean, well-documented, and easy to contribute to.</Highlight>{" "}
        Building in public done right.
      </p>
    ),
  },
];

export default function Testimonials() {
  return (
    <Section
      title="Testimonials"
      subtitle="Trusted by engineering teams worldwide"
      className="max-w-8xl"
    >
      <div className="relative mt-6 max-h-screen overflow-hidden">
        <div className="gap-4 md:columns-2 xl:columns-3 2xl:columns-4">
          {Array(Math.ceil(testimonials.length / 3))
            .fill(0)
            .map((_, i) => (
              <Marquee
                vertical
                key={testimonials[i * 3]?.name}
                className={cn({
                  "[--duration:60s]": i === 1,
                  "[--duration:30s]": i === 2,
                  "[--duration:70s]": i === 3,
                })}
              >
                {testimonials.slice(i * 3, (i + 1) * 3).map((card) => (
                  <motion.div
                    key={card.name}
                    initial={{ opacity: 0 }}
                    whileInView={{ opacity: 1 }}
                    viewport={{ once: true }}
                    transition={{
                      delay: Math.random() * 0.8,
                      duration: 1.2,
                    }}
                  >
                    <TestimonialCard {...card} />
                  </motion.div>
                ))}
              </Marquee>
            ))}
        </div>
        <div className="pointer-events-none absolute inset-x-0 bottom-0 h-1/4 w-full bg-gradient-to-t from-background from-20%" />
        <div className="pointer-events-none absolute inset-x-0 top-0 h-1/4 w-full bg-gradient-to-b from-background from-20%" />
      </div>
    </Section>
  );
}
