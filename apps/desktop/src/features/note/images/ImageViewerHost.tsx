/** The viewer for the open note's images, driven by `useImageUi`. */
import { ImageViewer } from "@/ui";
import { useImageUi } from "./store";

export function ImageViewerHost() {
  const viewer = useImageUi((s) => s.viewer);
  const setIndex = useImageUi((s) => s.setViewerIndex);
  const close = useImageUi((s) => s.closeViewer);
  return (
    <ImageViewer
      open={!!viewer}
      images={viewer?.images ?? []}
      index={viewer?.index ?? 0}
      onIndexChange={setIndex}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    />
  );
}
