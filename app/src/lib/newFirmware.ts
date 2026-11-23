// AJAZZ's newest AK980 PRO firmware ("AK980 Led v2 Firmware 769", 3.01)
// moves the keyboard to another command set. Keymotion doesn't speak it yet,
// so the backend lists such a keyboard as unknown and sends it nothing; the
// page tells the user what it is (Welcome, the pane on the left, the tray).
//
// The new firmware shows up as 0C45:8039, and AJAZZ's own tools also find
// it under 0x8009, 0x8029 and 0x8802. What tells it apart from the old
// firmware on those ids is its screen channel, a collection with usage page
// 0xFF67, which the old firmware never has. The collections and the USB
// release number come from the keyboard's support details (read from
// Windows; nothing is sent to the keyboard).
//
// No imports: tools/new-firmware.test.mjs runs this file in Node.

/** What a keyboard's support details say (lib/ipc.ts `unsupportedReport`). */
export interface KeyboardDetails {
  /** The USB release number (BCD), 0x0301 for 3.01. */
  release: number | null;
  /** The usage page of each collection. */
  pages: number[];
}

/** The new firmware's screen channel. */
export const NEW_FIRMWARE_PAGE = 0xff67;

/** The release number and usage pages from the support details' text
 *  ("USB ids: VID 0C45, PID 8009, release 0x0301", "usage page 0xFF67"). */
export function parseDetails(text: string): KeyboardDetails {
  const release = /release 0x([0-9A-F]{4})/i.exec(text);
  const pages = Array.from(text.matchAll(/usage page 0x([0-9A-F]{4})/gi), (m) => parseInt(m[1]!, 16));
  return { release: release ? parseInt(release[1]!, 16) : null, pages };
}

const ids = (id: string) => id.toUpperCase().split(":");

/** A SONiX-made keyboard that calls itself AK980 PRO: its details tell
 *  whether it runs the new firmware. */
export function isAk980Candidate(k: { id: string; name: string | null }): boolean {
  return ids(k.id)[0] === "0C45" && /AK980\s*PRO/i.test(k.name ?? "");
}

/** An unknown keyboard is an AK980 PRO on AJAZZ's new firmware: 0C45:8039
 *  always, any other id when its collections include usage page 0xFF67. */
export function isNewAk980Firmware(k: { id: string; name: string | null }, details?: KeyboardDetails | null): boolean {
  if (!isAk980Candidate(k)) return false;
  if (ids(k.id)[1] === "8039") return true;
  return !!details?.pages.includes(NEW_FIRMWARE_PAGE);
}

/** "3.01" from the release number 0x0301 (binary-coded decimal). */
export function firmwareVersion(release: number | null | undefined): string | null {
  if (release == null) return null;
  return `${(release >> 8).toString(16)}.${(release & 0xff).toString(16).padStart(2, "0")}`;
}
