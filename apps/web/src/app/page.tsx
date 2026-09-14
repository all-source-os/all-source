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
// Public HTML is cacheable; live prices load after paint from the catalog proxy.
// Keep the freshness bound used by other bet marketing sites.
export const revalidate = 300;

export default function Home() {
  return (
    <main className="marketing-theme dark relative min-h-screen overflow-hidden bg-background text-foreground">
      <EarlyAccessBanner />
      <Header />
      <Hero />
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
      <Pricing />
      <FAQ />
      <Blog />
      <CTA />
      <Footer />
    </main>
  );
}
