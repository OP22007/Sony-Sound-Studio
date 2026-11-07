# Device directory

Accepted 2026-11-07: searchable static directory and individual model pages, with a shared registry generating the website and GitHub compatibility reference.

Audience: prospective users choosing a download and testers reporting model-specific results. Scope: the three explicit Sony profiles and 30 OnePlus catalog names (32 product IDs), including regional aliases. Generic name discovery is not a model-specific implementation.

Decision: maintain `docs/compatibility/devices.json` as the public status registry. Generate static HTML and Markdown; no database, accounts, tracking or runtime API. Keep all devices readable without JavaScript. Local filters progressively enhance the directory. This preserves GitHub Pages hosting, fast navigation and crawlable model URLs. Alternatives considered: one large table (crowded), or community database (unnecessary hosting and moderation).

Each feature has a platform-specific status, explanation and evidence reference. Confirmation applies only to its recorded firmware, host and app version. Missing dates/versions stay explicitly unknown. Review dates are never substituted for test dates. A missing mapping is Unknown, not proof of hardware absence. Source-build features are distinguished from the 0.1.3 released baseline.

Contributions use prefilled GitHub reports and maintainer-reviewed registry pull requests. Reports must exclude Bluetooth addresses, serial numbers, pairing secrets and personal media metadata. Build validation rejects unknown statuses, duplicate identities, missing feature/platform cells and confirmation without hardware evidence. Generated documentation must match the registry. Adding a new status claim requires evidence review; community reports do not automatically confer confirmation.

Initial limitations: incomplete hardware matrix, many untested models, macOS control disabled, several catalog names may not be retail products. No device settings are sent by the website. Artwork is exact-model vendor art where available; missing art is labelled rather than substituted.

Scope correction, 2026-11-07: expand Sony beyond the three explicit desktop profiles to all 62 models in Sony USA’s September 2026 Sound Connect list. Include headphones/earphones, gaming and speakers with category and catalog-only availability filters. Catalog-only Windows/Linux feature support is Unknown; macOS controls remain Not implemented. OnePlus remains 30 catalog names. Total: 92 named devices.
