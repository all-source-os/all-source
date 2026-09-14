import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "Terms of Service",
  description: `Terms of Service for ${siteConfig.name} - the rules governing use of our platform.`,
  canonical: "/terms",
});

export default function TermsOfService() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <h1 className="text-3xl font-bold text-foreground sm:text-4xl mb-2">Terms of Service</h1>
      <p className="text-sm text-muted-foreground mb-10">Last updated: February 12, 2026</p>

      <div className="prose prose-neutral dark:prose-invert max-w-none space-y-8">
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">1. Acceptance of Terms</h2>
          <p className="text-muted-foreground leading-relaxed">
            By accessing or using AllSource (&quot;the Service&quot;), you agree to be bound by
            these Terms of Service. If you are using the Service on behalf of an organization, you
            represent that you have the authority to bind that organization to these terms.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">2. Description of Service</h2>
          <p className="text-muted-foreground leading-relaxed">
            AllSource provides an event store platform consisting of AllSource Core, Query Service,
            MCP servers, hosted control services, and a web dashboard. The Service enables event
            sourcing, temporal queries, stream processing, and AI-assisted data operations.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">3. Account Registration</h2>
          <ul className="list-disc pl-6 text-muted-foreground space-y-2">
            <li>You must provide accurate and complete registration information</li>
            <li>
              You are responsible for maintaining the confidentiality of your account credentials
              and API keys
            </li>
            <li>You are responsible for all activities that occur under your account</li>
            <li>You must notify us immediately of any unauthorized use of your account</li>
            <li>One person or entity may not maintain more than one concurrent trial account</li>
          </ul>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">4. Acceptable Use</h2>
          <p className="text-muted-foreground leading-relaxed mb-3">
            You agree not to use the Service to:
          </p>
          <ul className="list-disc pl-6 text-muted-foreground space-y-2">
            <li>Violate any applicable laws or regulations</li>
            <li>Infringe on the intellectual property rights of others</li>
            <li>Store or transmit malicious code, malware, or viruses</li>
            <li>Attempt to gain unauthorized access to the Service or other users&apos; data</li>
            <li>Interfere with or disrupt the Service or its infrastructure</li>
            <li>Circumvent usage limits, rate limiting, or billing mechanisms</li>
            <li>Resell or redistribute the Service without authorization</li>
            <li>Store regulated data (PHI, PCI) without an appropriate plan and agreement</li>
          </ul>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">5. Plans and Billing</h2>
          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">5.1 Plans</h3>
          <p className="text-muted-foreground leading-relaxed">
            The hosted Service is offered in paid tiers (Indie, Studio, and Scale) and a custom
            Enterprise tier. New accounts begin with a time-limited trial, after which a paid plan
            is required to continue using the hosted Service. Each tier has defined usage limits for
            events, streams, tenants, and retention periods as described on our pricing page. The
            open-source core may also be self-hosted under its license at no charge.
          </p>

          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">5.2 Billing</h3>
          <p className="text-muted-foreground leading-relaxed">
            Paid plans are billed monthly or annually in advance. Usage-based overages, if
            applicable, are billed at the end of each billing period. All fees are non-refundable
            except as required by law.
          </p>

          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">5.3 Trial</h3>
          <p className="text-muted-foreground leading-relaxed">
            New hosted accounts include a time-limited trial (currently 14 days) with limited
            resources so you can evaluate the Service. When the trial ends, a paid plan is required
            to continue. We reserve the right to modify trial limits or duration with 30 days&apos;
            notice.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">6. Your Data</h2>
          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">6.1 Ownership</h3>
          <p className="text-muted-foreground leading-relaxed">
            You retain all rights to the data you store in the Service. We do not claim ownership of
            your event data, schemas, projections, or any other content you create or upload.
          </p>

          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">6.2 License to Operate</h3>
          <p className="text-muted-foreground leading-relaxed">
            You grant us a limited license to store, process, and transmit your data solely for the
            purpose of providing and improving the Service.
          </p>

          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">6.3 Data Export</h3>
          <p className="text-muted-foreground leading-relaxed">
            You may export your data at any time using the API or MCP tools. Upon account
            termination, we will make your data available for export for 30 days before permanent
            deletion.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">7. Intellectual Property</h2>
          <p className="text-muted-foreground leading-relaxed">
            The Service, including its source code, documentation, design, and branding, is owned by
            AllSource and licensed under the Apache-2.0 License (enterprise components under BSL
            1.1) where applicable. The AllSource name, logo, and branding are trademarks of
            AllSource and may not be used without permission.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">8. Service Level</h2>
          <p className="text-muted-foreground leading-relaxed">
            We strive to maintain high availability but do not guarantee uninterrupted access.
            Planned maintenance will be announced in advance when possible. Enterprise customers may
            negotiate a separate Service Level Agreement (SLA) with defined uptime commitments and
            remedies.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">9. Limitation of Liability</h2>
          <p className="text-muted-foreground leading-relaxed">
            TO THE MAXIMUM EXTENT PERMITTED BY LAW, ALLSOURCE SHALL NOT BE LIABLE FOR ANY INDIRECT,
            INCIDENTAL, SPECIAL, CONSEQUENTIAL, OR PUNITIVE DAMAGES, OR ANY LOSS OF PROFITS, DATA,
            OR GOODWILL, ARISING OUT OF YOUR USE OF THE SERVICE. OUR TOTAL LIABILITY SHALL NOT
            EXCEED THE AMOUNT YOU PAID US IN THE 12 MONTHS PRECEDING THE CLAIM.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            10. Disclaimer of Warranties
          </h2>
          <p className="text-muted-foreground leading-relaxed">
            THE SERVICE IS PROVIDED &quot;AS IS&quot; AND &quot;AS AVAILABLE&quot; WITHOUT
            WARRANTIES OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING WARRANTIES OF MERCHANTABILITY,
            FITNESS FOR A PARTICULAR PURPOSE, AND NON-INFRINGEMENT. WE DO NOT WARRANT THAT THE
            SERVICE WILL BE ERROR-FREE OR UNINTERRUPTED.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">11. Indemnification</h2>
          <p className="text-muted-foreground leading-relaxed">
            You agree to indemnify and hold harmless AllSource from any claims, losses, or damages
            arising from your use of the Service, your violation of these Terms, or your violation
            of any third-party rights.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">12. Termination</h2>
          <p className="text-muted-foreground leading-relaxed">
            Either party may terminate the account at any time. We may suspend or terminate your
            access if you violate these Terms or engage in abusive behavior. Upon termination, your
            right to use the Service ceases immediately, and your data will be available for export
            for 30 days.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">13. Changes to Terms</h2>
          <p className="text-muted-foreground leading-relaxed">
            We may modify these Terms at any time. Material changes will be communicated via email
            or a prominent notice on the Service at least 30 days before taking effect. Continued
            use after changes constitutes acceptance.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">14. Governing Law</h2>
          <p className="text-muted-foreground leading-relaxed">
            These Terms are governed by the laws of the State of Delaware, United States, without
            regard to conflict of law principles. Any disputes shall be resolved in the state or
            federal courts located in Delaware.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">15. Contact</h2>
          <p className="text-muted-foreground leading-relaxed">
            For questions about these Terms, contact us at{" "}
            <a
              href={`mailto:${siteConfig.links.email}`}
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              {siteConfig.links.email}
            </a>
            .
          </p>
        </section>
      </div>
    </div>
  );
}
