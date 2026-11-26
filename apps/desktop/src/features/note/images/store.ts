/**
 * Shared state for images in the open note: which image's caption is being typed, whether the
 * toolbar shows the alt text field, which image is selected while the note (or its toolbar) has focus, which images just settled after a move, the viewer, and an image to reveal.
 */
import { create } from "zustand";
import type { ViewerImage } from "@/ui";

interface ImageUiState {
  /** Position of the image whose caption field should open. */
  captionAt: number | null;
  /** The toolbar shows the alt text field. */
  editingAlt: boolean;
  /**
   * Position of the selected image while the note or its toolbar has focus: the image shows as
   * selected only then, so clicking anywhere else (a sidebar, another pane) clears the look.
   */
  activeImage: number | null;
  /** Sources of images that just moved, so their new view plays the settle spring. */
  settling: Set<string>;
  viewer: { images: ViewerImage[]; index: number } | null;
  /** An image to select and scroll to once its note opens (a ⌘K image match). */
  reveal: string | null;
  editCaption: (pos: number | null) => void;
  setEditingAlt: (on: boolean) => void;
  setActiveImage: (pos: number | null) => void;
  markSettling: (src: string) => void;
  takeSettling: (src: string) => boolean;
  openViewer: (images: ViewerImage[], index: number) => void;
  setViewerIndex: (index: number) => void;
  closeViewer: () => void;
  revealImage: (src: string | null) => void;
}

export const useImageUi = create<ImageUiState>((set, get) => ({
  captionAt: null,
  editingAlt: false,
  activeImage: null,
  settling: new Set(),
  viewer: null,
  reveal: null,
  editCaption: (captionAt) => set({ captionAt }),
  setEditingAlt: (editingAlt) => set({ editingAlt }),
  setActiveImage: (activeImage) => {
    if (get().activeImage !== activeImage) set({ activeImage });
  },
  markSettling: (src) => {
    get().settling.add(src);
  },
  takeSettling: (src) => get().settling.delete(src),
  openViewer: (images, index) =>
    images.length > 0 ? set({ viewer: { images, index } }) : undefined,
  setViewerIndex: (index) => {
    const viewer = get().viewer;
    if (viewer) set({ viewer: { ...viewer, index } });
  },
  closeViewer: () => set({ viewer: null }),
  revealImage: (reveal) => set({ reveal }),
}));
