import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "Privacy Policy",
  description: `Privacy Policy for ${siteConfig.name} - how we collect, use, and protect your data.`,
  canonical: "/privacy",
});

export default function PrivacyPolicy() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <h1 className="text-3xl font-bold text-foreground sm:text-4xl mb-2">Privacy Policy</h1>
      <p className="text-sm text-muted-foreground mb-10">Last updated: August 27, 2026</p>

      <div className="prose prose-neutral dark:prose-invert max-w-none space-y-8">
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">1. Introduction</h2>
          <p className="text-muted-foreground leading-relaxed">
            AllSource (&quot;we&quot;, &quot;our&quot;, or &quot;us&quot;) operates the AllSource
            platform, including AllSource Core, Query Service, MCP servers, and web dashboard
            (collectively, the &quot;Service&quot;). This Privacy Policy explains how we collect,
            use, disclose, and safeguard your information when you use our Service.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">2. Information We Collect</h2>
          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">2.1 Account Information</h3>
          <p className="text-muted-foreground leading-relaxed">
            When you create an account, we collect your name, email address, and authentication
            credentials. If you sign up via OAuth (Google, GitHub), we receive your profile
            information from those providers.
          </p>

          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">2.2 Usage Data</h3>
          <p className="text-muted-foreground leading-relaxed">
            We collect metrics about your use of the Service, including event ingestion counts,
            query volumes, API call frequency, and storage usage. This data is used for billing,
            capacity planning, and service improvement.
          </p>

          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">2.3 Event Data</h3>
          <p className="text-muted-foreground leading-relaxed">
            You store event data in the AllSource event store. We do not access, analyze, or share
            the contents of your event data except as necessary to provide the Service (e.g.,
            executing your queries). Your event data remains yours.
          </p>

          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">2.4 Technical Data</h3>
          <p className="text-muted-foreground leading-relaxed">
            We automatically collect IP addresses, browser type, operating system, referring URLs,
            and device information when you interact with our web dashboard.
          </p>

          {/*
            Added for GEO layer 4 (docs/runbooks/GEO_MEASUREMENT.md). Section 2.3
            promises we do not read your event data — this is OUR telemetry, in our
            own tenant, and we DO read it, so it needs saying separately rather than
            being quietly covered by 2.2 "usage data".
          */}
          <h3 className="text-lg font-medium text-foreground mt-4 mb-2">
            2.5 Attribution Answers You Choose to Give Us
          </h3>
          <p className="text-muted-foreground leading-relaxed">
            During onboarding we ask an optional question — &quot;how did you find us?&quot; — and,
            if you say an AI assistant sent you, we offer an optional free-text box asking what you
            asked it. Both are entirely optional and skipping them has no effect on your account. If
            you answer, we store your answer (including the free text, verbatim, up to 500
            characters), the option you selected, your tenant ID, your plan at the time, and the
            timestamp. We never store your email address against these answers. The same two
            optional fields are available on our self-service <code>/api/v1/onboard/start</code>{" "}
            endpoint for programmatic signups.
          </p>
          <p className="text-muted-foreground leading-relaxed mt-2">
            Unlike the event data described in 2.3, these answers are our own operational data and{" "}
            <strong>are read by our team</strong>: we use them to work out which questions people
            ask AI assistants before finding us, and what to write next. They are not used for
            advertising, are not sold, and are not shared with third parties. Ask us and we will
            delete yours — see the Contact section below.
          </p>

          <h3
            id="design-partner-applications"
            className="scroll-mt-24 text-lg font-medium text-foreground mt-4 mb-2"
          >
            2.6 Design Partner Applications
          </h3>
          <p className="text-muted-foreground leading-relaxed">
            If you apply to the design partner program, we collect your name, work email, a short
            description of what you are building and its current memory problem, campaign
            attribution parameters, consent version, and submission time. We use this information
            only to assess fit, contact you about the program, support an accepted integration, and
            evaluate which campaign sources produce qualified applications.
          </p>
          <p className="text-muted-foreground leading-relaxed mt-2">
            Application details are stored in a private administrative event stream. We do not put
            applicant contact details or answers in public analytics properties, campaign URLs,
            GitHub issues, or public event streams. Rejected and waitlisted applications receive a
            retention deadline 90 days after the decision. Accepted applications receive a retention
            deadline 90 days after the 60-day program. You can request earlier removal using the
            contact address below.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            3. How We Use Your Information
          </h2>
          <ul className="list-disc pl-6 text-muted-foreground space-y-2">
            <li>Provide, operate, and maintain the Service</li>
            <li>Process billing and enforce usage quotas</li>
            <li>
              Send transactional emails (account verification, billing receipts, security alerts)
            </li>
            <li>Monitor service health and prevent abuse</li>
            <li>Improve the Service based on aggregate usage patterns</li>
            <li>Respond to support requests</li>
            <li>Comply with legal obligations</li>
          </ul>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">
            4. Data Storage and Security
          </h2>
          <p className="text-muted-foreground leading-relaxed">
            Your event data is stored in the AllSource Core engine with write-ahead logging (WAL)
            and Parquet columnar storage. All data is encrypted in transit (TLS 1.2+). We implement
            industry-standard security practices including multi-tenancy isolation, role-based
            access control (RBAC), and comprehensive audit logging.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">5. Data Retention</h2>
          <p className="text-muted-foreground leading-relaxed">
            Event data retention depends on your plan tier (7 days for Developer, 90 days for Team,
            unlimited for Enterprise). Account information is retained while your account is active
            and for a reasonable period afterward for legal and operational purposes. You may
            request deletion of your account and associated data at any time.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">6. Data Sharing</h2>
          <p className="text-muted-foreground leading-relaxed">
            We do not sell your personal information. We may share data with:
          </p>
          <ul className="list-disc pl-6 text-muted-foreground space-y-2 mt-2">
            <li>
              <strong>Service providers</strong> who assist in operating the Service (payment
              processing, email delivery, infrastructure hosting)
            </li>
            <li>
              <strong>Legal authorities</strong> when required by law, court order, or governmental
              regulation
            </li>
            <li>
              <strong>Business transfers</strong> in connection with a merger, acquisition, or sale
              of assets
            </li>
          </ul>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">7. Your Rights</h2>
          <p className="text-muted-foreground leading-relaxed">
            Depending on your jurisdiction, you may have the right to:
          </p>
          <ul className="list-disc pl-6 text-muted-foreground space-y-2 mt-2">
            <li>Access the personal data we hold about you</li>
            <li>Request correction of inaccurate data</li>
            <li>Request deletion of your data</li>
            <li>Export your data in a portable format</li>
            <li>Object to or restrict processing of your data</li>
            <li>Withdraw consent at any time</li>
          </ul>
          <p className="text-muted-foreground leading-relaxed mt-3">
            To exercise these rights, contact us at{" "}
            <a
              href={`mailto:${siteConfig.links.email}`}
              className="text-foreground underline underline-offset-4 hover:opacity-80"
            >
              {siteConfig.links.email}
            </a>
            .
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">8. Cookies</h2>
          <p className="text-muted-foreground leading-relaxed">
            We use essential cookies for authentication and session management. Google Analytics 4
            receives cookieless measurement pings with analytics and advertising storage denied by
            default. We remove URL query strings and referrer query strings, disable Google Signals
            and advertising personalisation, and do not set advertising cookies. Enhanced
            measurement covers aggregate interactions such as scrolls, outbound links, forms,
            videos, and downloads; automatic browser-history page views and site-search capture are
            disabled.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">9. Children&apos;s Privacy</h2>
          <p className="text-muted-foreground leading-relaxed">
            The Service is not directed at children under 16. We do not knowingly collect personal
            information from children. If you believe a child has provided us with personal data,
            please contact us for removal.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">10. Changes to This Policy</h2>
          <p className="text-muted-foreground leading-relaxed">
            We may update this Privacy Policy from time to time. We will notify you of material
            changes by email or by posting a notice on the Service. Your continued use of the
            Service after changes constitutes acceptance of the updated policy.
          </p>
        </section>

        <section>
          <h2 className="text-xl font-semibold text-foreground mb-3">11. Contact Us</h2>
          <p className="text-muted-foreground leading-relaxed">
            If you have questions about this Privacy Policy, contact us at{" "}
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
