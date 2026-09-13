import { EarlyAccessBanner } from "@/components/early-access-banner";
import Blog from "@/components/sections/blog";
import CTA from "@/components/sections/cta";
import FAQ from "@/components/sections/faq";
import Features from "@/components/sections/features";
import Footer from "@/components/sections/footer";
import Header from "@/components/sections/header";
import Hero from "@/components/sections/hero";
import HowItWorks from "@/components/sections/how-it-works";
import Pricing from "@/components/sections/pricing";
import Problem from "@/components/sections/problem";
import SocialProof from "@/components/sections/social-proof";
import StatStrip from "@/components/sections/stat-strip";
import { indiePrice as defaultIndiePrice } from "@/lib/config";
import { fetchCatalog, indexByTier } from "@/lib/pricing-catalog";

// Revalidate live LemonSqueezy prices hourly (ISR).
export const revalidate = 3600;

export default async function Home() {
  // Live LemonSqueezy prices (source of truth) for the hero CTA + pricing cards.
  const catalog = await fetchCatalog();
  const indiePrice = indexByTier(catalog).indie?.monthly?.formatted ?? defaultIndiePrice;

  return (
    <main className="marketing-theme dark relative min-h-screen overflow-hidden bg-background text-foreground">
      <EarlyAccessBanner />
      <Header />
      <Hero indiePrice={indiePrice} />
      {/* Stats demoted below the fold — final values painted, never "0K" flash */}
      <StatStrip />
      {/* Logos section hidden - needs real partner logos */}
      {/* <Logos /> */}
      <Problem />
      <HowItWorks />
      {/* Testimonials carousel hidden - no real testimonials yet */}
      {/* <TestimonialsCarousel /> */}
      <Features />
      <SocialProof />
      <Pricing catalog={catalog} />
      <FAQ />
      <Blog />
      <CTA />
      <Footer />
    </main>
  );
}
