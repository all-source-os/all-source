import { ChangelogSignup } from "@/components/changelog-signup";
import Footer from "@/components/sections/footer";
import HarnessHome from "@/components/sections/harness-home";
import styles from "@/components/sections/harness-home.module.css";
import Header from "@/components/sections/header";
import Hero from "@/components/sections/hero";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "Agent Memory on an Event Store | AllSource Prime",
  description:
    "Give your coding agent a memory of how your company works. Connect services to owners and decisions in AllSource Prime, built on a durable event store, and find the source in a fresh session.",
  canonical: "/",
});

export default function Home() {
  return (
    <main className={`marketing-theme dark relative min-h-screen ${styles.page}`}>
      <Header homepage />
      <div className={styles.canvas}>
        <Hero />
        <HarnessHome />
      </div>
      <div className={styles.news}>
        <ChangelogSignup placement="home" />
      </div>
      <Footer />
    </main>
  );
}
