import { createFileRoute } from "@tanstack/react-router";

import { Champions } from "@/pages/Champions";

export const Route = createFileRoute("/champions")({
  component: Champions,
});
