# Live preservation policy

Protocol 1 preserves neutral legacy documents. Protocol 2 adds authoritative native pixels and document color. Protocol 3 preserves the complete common and Motion reader/model through document feature 27. Protocol 3 is not a catch-all for future data. Layout 28 is unsupported.

The backend stores encoded cells without materializing artwork. HTTP and Convex share `site/shared/liveProtocol.ts`. Rust uses `collaboration::protocol`, the existing `Replica::retained_cells()` traversal and the existing resource subtree visitor. The common JSON fixture file is consumed by both policy test suites. Original `/sources` payloads remain opaque. Historical pixel cells remain encoded.

The local advertised maximum comes from `collaboration::supported_protocol()`: every reader 1–27 must exist, and a typed `Motion` round trip must retain expressions, a composition's actual document, Hold/CubicBezier keys and a real scene camera. The result is cached. This source still implements readers 1–24 and advertises **2**. A caller passing `document_for_protocol(..., 3)` cannot raise the compiled reader limit. Known common-only payloads requiring 3 are deliberately unavailable in this 24-only build until the complete reader lands. Record, receive, transport and materialization refuse before changing local state.

## Retained predicate matrix

Historical peer: `f52dbac87350c9ad35b064d5366bc3c2e954e938`. Common model: `18c97331ea7c797a8622d5ee778a6c6b85175592`. Motion model shape inspected at `1e44d281`. These are source findings; they do not replace their exact historical execution receipts.

| Actual semantic slot | Requirement 3 predicate | Historical protocol 2 omission |
| --- | --- | --- |
| Document `publishing` | `facing_pages=true`, nonzero bleed, nonempty styles/masters/inherited layers, or retained map entries | `Document` has no publishing field; publishing references on text are also absent |
| Document `sources` | Nonempty descriptor vector or any retained descriptor child | `Document` has no original-source archive; descriptor bytes are never parsed by this classifier |
| Layer `svg_filter`, `source_attachment`, `psd_type`, `psd_smart` | Any retained `Some` value or child property | Fields absent from old `Layer`; ordinary struct serde ignores them |
| `geom/Text/transform`, `geom/Text/svg_text` | Any retained `Some`, including an explicit identity transform | Fields absent from old `TypeRun` |
| `geom/Text/font` | Nonempty actual font string, even when currently visible; empty/default stays legacy | Field exists, but old `typography::document_fonts` visits current artwork only; later deletion/withdrawal can lose a retained-only face on cold save/recovery |
| `geom/Text/spans/N/{publishing_style,font,px,fill}` | Any retained `Some`, including explicit empty font or `Fill::None` | Fields absent from old `CharSpan`; old archive font collection also cannot find span-only faces |
| `geom/Text/paragraphs/N/publishing_style` | Any retained `Some` | Field absent from old `ParagraphStyle` |
| Shape `style/fill/Pattern`, span `fill/Pattern` | Variant, atomic value, or any child/definition property | Old `Fill` enum refuses Pattern explicitly; the complete reader/archive is still required |
| Fill `Gradient` or stroke `gradient` | `radial_axis=Some`, `spread=Repeat/Reflect`, nonzero `focal` or `inner_radius`; includes individual component writes | Those fields absent from old `Gradient`; old typed artwork silently loses radial/spread state |
| Motion `expressions` | Nonempty array or retained child | Feature 25 absent from old typed Motion |
| Motion `compositions` and stable `/@compositions/{id}/document` | Nonempty array, child, or stable child document cell | Feature 26 absent; includes nested authoritative pixels |
| Motion track/time-remap ease | Hold or CubicBezier, including variant marker and control-point writes | Ease variants absent from old enum |
| Motion `scene3d` | Nonnull retained scene or child | Feature 27 absent from old typed Motion |

Empty/default containers, absent/None optionals, default publishing, Pad/zero/default gradient fields, ordinary text spans and empty/default Motion remain 1. Native pixels or nonneutral color raise those controls to 2. A protocol-3 offer alone never raises a room requirement.

Matching uses native document, layer, text, fill and gradient addresses and serialized subtrees/arrays. Arbitrary names, SVG/XML strings and opaque original bytes are not interpreted as features. Unknown cell variants, document root properties, Motion/publishing/gradient properties, easing/spread/fill variants and future declared envelopes refuse. The policy is a capability classifier, not a replacement for typed document/resource validation; it does not assign 3 to arbitrary malformed JSON.

## Negotiation and historical rooms

`protocol` on a room/reply is the monotonic required minimum. `supportedProtocol` is the negotiated offer: offered 2 gets exactly 2; offered 3 gets 3. Legacy omitted/1 callers retain the prior supported-2 response on neutral rooms. Every exchange, including an empty poll, rechecks the room requirement before actor, sequence, revision, presence, log, acknowledgement or requirement updates. A failed transaction rolls back its entire prefix and any upgrade. Undo, removal and withdrawn writes never reduce the requirement.

HTTP performs read-only offer/seed preflight **before** quota initialization. The authoritative create mutation repeats it before any quota or room insertion. Quota accounting retains the existing bounded metadata counter.

New rooms carry optional `protocolPolicy=3` on the existing room row. Older rooms are never trusted merely because they store protocol 1/2: join/exchange must inspect the actual seed and every retained operation. A single read-only page (at most 8 rows / 2,000,000 row bytes) can prove a small room complete. Otherwise the operation refuses without mutation. An explicit authenticated `live:certify` action scans bounded read-only pages, then commits the single sticky requirement only after proving one unchanged revision across the whole scan. Failure, an old offer, a changed revision, unknown data or the 256-page ceiling leaves the room unmodified and closed. There is no partial certification state, second store or compaction. A room beyond that bounded certification window remains unsupported/closed; an empty latest page is never treated as proof of its earlier history.

Rust validates reply metadata and the retained incoming cells before returning acknowledgement/presence to the application. Protocol-2 legacy replies without added metadata remain usable by the 24-only reader; a full protocol-3 reader requires explicit metadata. Current-application document replacement remains staged by the existing live adapter.

## Proof boundary

This cut supplies source, shared policy fixtures and isolated Node/TypeScript checks. Rust fixtures cover local-reader refusal/atomicity, retained classification, known common refusal and reply/poll gates, but are **not executed** in this cut. Existing native pixels, decoded 512 MiB transport quota, one-pool admission, cold archives, author-owned undo and two-process HTTP tests remain unchanged apart from replacing the app test's protocol constant with the advertised-capability function. No backend was deployed and no native/service/product job was started.

Qualification must use the actual complete common+Motion union, its updated isolated backend and one coherent serialized headless library group. It must retain the original native/archive/app acceptance (archive capability assertions now require 3 and explicitly reject 2) and additionally prove real common+Motion save/recovery/reopen and process round trips. Passing these pure policy checks alone does not qualify protocol 3.
