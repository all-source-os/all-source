# E2E Testing with Playwright

End-to-end testing infrastructure for the AllSource Event Store project.

## Overview

This package provides comprehensive E2E testing using Playwright with clean architecture principles:

- **Page Object Models (POM)** - Maintainable page abstractions
- **Custom Fixtures** - Reusable test dependencies
- **Test Helpers** - Data builders and utilities
- **Organized Test Structure** - Tests grouped by feature

## Structure

```
tooling/e2e/
├── package.json              # Package configuration
├── playwright.config.ts      # Playwright configuration
│
├── page-objects/             # Page Object Models
│   ├── BasePage.ts          # Base class with common functionality
│   ├── UITestPage.ts        # UI test page interactions
│   └── index.ts             # Exports
│
├── fixtures/                 # Custom Playwright fixtures
│   └── pages.ts             # Page object fixtures
│
├── helpers/                  # Test utilities
│   └── test-data.ts         # Data builders and generators
│
└── tests/                    # Test suites
    ├── demo/                # Demo page tests
    │   └── navigation.spec.ts
    └── ui-components/       # UI component tests
        └── components.spec.ts
```

## Installation

```bash
# From the monorepo root
cd tooling/e2e
bun install

# Install Playwright browsers
bunx playwright install
```

## Running Tests

```bash
# Run all tests
bun test

# Run tests in headed mode
bun test:headed

# Run tests with UI mode
bun test:ui

# Run tests in debug mode
bun test:debug

# Run specific test suite
bun test:demo
bun test:ui-components

# Show test report
bun test:report
```

### Against a production web build

These configs start `apps/web` with `next start`, so build it first.

```bash
# SEO content checks
(cd ../../apps/web && bun run build)
bunx playwright test --config playwright.seo.config.ts

# Exception capture: PostHog must be configured at build time; requests are intercepted locally
(cd ../../apps/web && NEXT_PUBLIC_POSTHOG_KEY=phc_e2e_local bun run build)
bunx playwright test --config playwright.analytics.config.ts
```

## Writing Tests

### Using Page Objects

```typescript
import { test, expect } from "../../fixtures/pages";

test("should navigate to demo", async ({ demoPage }) => {
  await demoPage.goto();
  await demoPage.waitForPageLoad();
  await demoPage.navigateToEvents();
  // ... test assertions
});
```

### Using Test Data Builders

```typescript
import { createTestEvent, createEcommerceEvent } from "../../helpers/test-data";

test("should create event", async ({ page }) => {
  const event = createEcommerceEvent("OrderPlaced");
  // ... use event data
});
```

## Page Object Model Pattern

Each page object extends `BasePage` and provides:

- **Navigation methods** - `goto()`, `waitForPageLoad()`
- **Interaction methods** - Button clicks, form fills
- **Verification methods** - Check visibility, get text
- **Element locators** - Centralized selectors

Example:

```typescript
export class MyPage extends BasePage {
  private readonly submitButton: Locator;

  constructor(page: Page) {
    super(page);
    this.submitButton = page.getByRole("button", { name: "Submit" });
  }

  async goto(): Promise<void> {
    await this.page.goto(`${this.baseURL}/my-page`);
  }

  async waitForPageLoad(): Promise<void> {
    await this.submitButton.waitFor({ state: "visible" });
  }

  async submit(): Promise<void> {
    await this.submitButton.click();
    await this.waitForNetworkIdle();
  }
}
```

## Configuration

### Environment Variables

Create `.env` in the tooling/e2e directory:

```env
BASE_URL=http://localhost:3000
```

### Browser Configuration

Edit `playwright.config.ts` to configure:

- Browsers to test (Chromium, Firefox, WebKit, Mobile)
- Test timeout and retries
- Screenshot and video settings
- Reporter options

## Best Practices

1. **Use Page Objects** - Avoid direct page interactions in tests
2. **Use Fixtures** - Inject dependencies via custom fixtures
3. **Use Test Data Builders** - Generate test data with helper functions
4. **Descriptive Test Names** - Write clear test descriptions
5. **Organize by Feature** - Group related tests in directories
6. **Wait for Actions** - Use `waitForNetworkIdle()` after interactions
7. **Accessibility** - Use `getByRole()` and semantic selectors

## CI/CD Integration

Tests are configured to run in CI with:

- Automatic retries on failure
- Video recording on failure
- Screenshot on failure
- JSON and HTML reports

## Debugging

```bash
# Debug mode with Playwright Inspector
bun test:debug

# Interactive UI mode
bun test:ui

# Generate code from browser interactions
bun codegen http://localhost:3000
```

## Common Issues

### Test Timeout

Increase timeout in `playwright.config.ts`:

```typescript
use: {
  timeout: 30000, // 30 seconds
}
```

### Flaky Tests

- Use `waitForNetworkIdle()` after interactions
- Use explicit waits instead of timeouts
- Check for element visibility before interactions

### Browser Not Found

```bash
bunx playwright install
```
