# Issue 157 — inline agent attachments

[Native E2E screen recording](agent-attachments.mp4) · [Result and hashes](result.json) · [Restored conversation](restored-history.png)

The 35-second, 1600×900 H.264 recording contains 350 actual native WGPU framebuffer frames at 10 fps. All assertions passed: **zero unresolved input targets/errors**, nine attachments delivered, unchanged canvas document, and the same nine attachments restored from saved History. See [the recorder log](native.log) and [empty error list](agent-attachments-errors.json).

Recorded source: `e382818602ad962d815c23de631fd747dc943767`. This is the complete prepared issue stack through area text, including the final path arc-accuracy and manual-ligature fixes, plus the attachment History correction. The recording uses the updated icon/zoom chrome. Later typography inspector width changes do not affect this attachment workflow. The executable SHA-256 is `cb2dec59eca9ea752de0a21458c504d3dff273f403928348340dfe328a044beb`; attachment-related source file hashes are in `result.json`.

## Recorded workflow

| Time | Input and result |
|---|---|
| 0–4 s | Focus the composer, paste short text inline, then paste a 60-line log as one token and chip. |
| 4–8 s | Paste a native image-only PNG clipboard with **no `Event::Paste`**, then SVG markup. |
| 8–14 s | Remove SVG atomically with Backspace, paste it again, remove its chip with ×, and paste it again at the cursor. |
| 15–21 s | Paste a native file URI list containing PDF, video, audio, and text; then copied Omadesign objects; then unknown MIME bytes. The prompt contains nine unique mapped references. |
| 21–25 s | Scroll the chip list to inspect the mixed file and object references, with the composer and Send button still visible. |
| 25–28 s | Send through the real ACP runtime to the local fixture peer, which acknowledges receipt. |
| 28–35 s | Click New, open History, expand the persisted conversation, and click Continue in saved document. All nine chips and the previous response are restored. |

The clipboard fixtures are placed on the **real Wayland clipboard** with their offered MIME types. The recorder injects pointer/key/paste events through egui's native input hook; the production clipboard reader, ingestion workers, composer, persistence writer, and ACP runtime handle the flow. Images are read from the OS clipboard without a text paste event. Only initial artwork and the local ACP connection are seeded. Frames are captured from the actual application viewport and encoded by ffmpeg; deterministic replay time is not a real-time performance measurement.

## Delivery and persistence checks

The [raw received ACP payload](acp-received.json) contains 11 text blocks (guidance and reference notes), six embedded resources, two PNG image blocks, one audio block, and one video resource link. The two decoded PNGs are 240×160 and 960×640. All base64 validates; encoded binary content totals 9,452 bytes. Every attachment's visible token occurs in the sent user text, IDs are unique, and every source file exists.

The thread written by the application's persistence worker was read back independently and its user text and attachment metadata matched the [restored conversation](conversation.json). [Reference copies](references/) and their hashes are included for inspection; metadata preserves the original synthetic recording paths. The [editable native document](attachment-reference.oma) remained byte-identical under document serialization before and after all chat operations.

## Regression coverage

- [12 attachment-focused tests](attachment-tests.log) pass, including short/long/native-file conversion, image and object payloads, legacy transcript compatibility, missing-file metadata, file/count/base64/typed-text limits, Unicode atomic deletion through the actual TextEdit, rejected-paste draft preservation, FIFO worker insertion, replacing an attachment at the 20-item cap, canvas isolation and agent-panel drops, and compact/full composer and History layout.
- [22 agent and clipboard tests](agent-tests.log) passed on the complete prepared stack before the final History-only layout fix. The subsequent focused run includes its new History regression. The ACP integration test exercises real subprocess transport for **no capabilities**, **image only**, and **all capabilities**, including unsupported-type file/path fallback.
- The local fixture peer is in [fake-agent.py](fake-agent.py). No paid provider turn is needed for this proof; named third-party provider services were not independently exercised.

After the recorded run, commit `04638556c2e79758f9db222038ceab3dec36fbc2` additionally preserves both the unsent prompt and a concurrently edited next draft when preparation fails, is stopped, or is cancelled during connection. Pending paste positions move with the restored prefix. [All 13 attachment tests pass](recovery-tests.log), including those three recovery branches. This changes interruption recovery only; the recorded successful delivery and restore path is unchanged.

The recording uses an isolated XDG profile and a synthetic document. It does not modify the installed stable application or accepted QA evidence.

Legacy copied-object attachments now use the same version-aware appearance migration as canvas paste. The regression compares the persisted filters and exact preview PNG against migration of the equivalent version-6 document; [all four ingestion tests pass](legacy-preview-tests.log). The recorded modern attachment workflow is unchanged.
