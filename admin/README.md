# Sound Studio owner admin

A separate Vercel application for reviewing device compatibility. The public website remains on GitHub Pages. Every private page and API checks the authenticated GitHub numeric account ID **140067318 (OP22007)** on the server. Changing a browser value, GitHub display name or URL does not grant access. There is no shared password, public signup or browser-stored GitHub token.

## Deploy once

1. Import this repository into a **separate Vercel project**. Set the production Git branch to `codex/website-scroll-launch` until the website PR is merged. Set **Root Directory** to `admin`, Framework Preset to **Other**, Node.js to **24.x**, and use the included `vercel.json`. There are no dependencies or build steps. Keep the project URL stable.
2. In [GitHub OAuth Apps](https://github.com/settings/developers), create an OAuth app with your Vercel project URL as Homepage URL, and `https://YOUR-ADMIN.vercel.app/auth/callback` as Authorization callback URL. Keep this app owned by your account. Create a client secret.
3. Add `ADMIN_ORIGIN`, `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` and `SESSION_SECRET` to the Vercel project's **Production** environment variables. `ADMIN_ORIGIN` is the exact HTTPS origin, without a trailing path. Generate `SESSION_SECRET` with `node -e "console.log(require('node:crypto').randomBytes(32).toString('hex'))"`. Keep credentials in Vercel, not source code, chat or client-side variables. `SOURCE_BRANCH` defaults to the existing website working branch.
4. Deploy/redeploy. Visit the project URL and choose **Sign in with GitHub** as OP22007. The OAuth app requests `public_repo` to commit to this public repository. This OAuth scope covers your public repositories; the server itself fixes all writes to `OP22007/Sony-Sound-Studio`. A GitHub App would be an alternative if repository-scoped installation permissions are required later.
5. Before relying on it, sign out and test with a different GitHub account: access must be denied. Verify one real owner review and publication. Automated tests use a mocked GitHub service; live OAuth/Vercel validation requires your credentials and deployment.

GitHub Pages cannot host this private server. Do not copy the admin app into the Pages build. Vercel preview deployments require a separate OAuth app/origin and secret configuration; do not share production secrets with untrusted previews. Protect your GitHub account with 2FA/passkeys.

## Review and publish

Search for a model, select Windows/macOS/Linux and choose a feature. Set the result and public note. Add each result to the review; then **Save review to GitHub**. Confirmed working additionally requires firmware, app version/commit, OS version, test date, steps, requested value, readback and physical/audible observation. Each feature's evidence carries its own test context; changing one feature does not overwrite another test or a different model's inherited defaults.

Saving creates one commit containing `devices.json`, the generated GitHub compatibility table and `admin-tests.md`. Old test records remain for history. It does not change code, release availability or protected `main`. It writes to `SOURCE_BRANCH` (restricted to a `codex/` working branch). While working locally, fetch/pull that branch before making further changes after an admin save.

**Publish saved compatibility** generates the same model pages and directory as the website build and commits those pages to `gh-pages`. It preserves homepage assets, artwork and sitemap, and writes `compatibility-version.json` with the source commit. Publication requests a Pages build directly and does not depend on ordinary Actions minutes. A successful commit/build request is not confirmation that GitHub finished deploying. Check the public page and compare `compatibility-version.json` with the displayed source commit. If GitHub Pages is configured for branch deployment, retain `gh-pages` as its source.

Reload before retrying a source/Pages conflict. Saves and publication are separate operations so a failed publication does not silently lose a saved review. If the network times out during a commit, check GitHub history before retrying. This editor changes existing models' summaries and per-feature test results; adding models, artwork or release implementation remains a code review task.

## Security and validation

AES-256-GCM sealed, purpose-bound, four-hour sessions in HttpOnly/Secure/SameSite cookies; OAuth state and S256 PKCE; fixed callback origin; owner ID validation against GitHub on each private request; same-origin and session CSRF checks for writes; bounded/allowlisted edits; escaped public rendering; non-forced Git branch updates; no-store responses; CSP/frame protection. Tokens and upstream payloads are not logged or returned to JavaScript. Access fails closed if deployment secrets are missing. Rotate `SESSION_SECRET` and redeploy to invalidate all sessions; revoke OAuth access from GitHub when needed. Sign-out clears the current browser cookie; copied cookies remain usable until expiry or key rotation.

Tests: `npm --prefix admin test`. Local development: copy `.env.example` to `.env`, use a separate local OAuth app with `http://127.0.0.1:4330/auth/callback`, set that local origin, and run `npm --prefix admin run dev`. There is no development login bypass. Node 24 is required.

Implementation references: [GitHub OAuth web flow and PKCE](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps), [Git trees](https://docs.github.com/en/rest/git/trees), [Git refs](https://docs.github.com/en/rest/git/refs), [Vercel Node functions](https://vercel.com/docs/functions/runtimes/node-js).
