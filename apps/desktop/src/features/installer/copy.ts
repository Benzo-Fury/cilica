/** The words for each of Cilica Setup's steps: while it runs, and the line it leaves when done. */
import type { Step } from "@/ipc/installer/client";

export const STEP_WORDS: Record<Step, { doing: string; done: string }> = {
  check: { doing: "Checking there's room on this PC", done: "There's room on this PC" },
  unpack: { doing: "Unpacking Cilica", done: "Unpacked Cilica" },
  copy: { doing: "Copying Cilica", done: "Copied Cilica and added it to the Start menu" },
  shortcut: { doing: "Adding a desktop shortcut", done: "Added a desktop shortcut" },
  sign_in: { doing: "Setting Cilica to open when you sign in", done: "Opens when you sign in" },
  register: { doing: "Adding Cilica to Installed apps", done: "Listed in Installed apps" },
  remove_files: {
    doing: "Removing Cilica's files and shortcuts",
    done: "Removed Cilica's files and shortcuts",
  },
  delete_data: {
    doing: "Deleting notes, recordings and settings",
    done: "Deleted notes, recordings and settings",
  },
  delete_keys: { doing: "Removing saved API keys", done: "Removed saved API keys" },
  unregister: { doing: "Removing Cilica from Installed apps", done: "Removed from Installed apps" },
};
