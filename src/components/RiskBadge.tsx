import { Badge } from "@/components/ui/badge";
import type { Risco } from "@/lib/api";
import { ROTULO_RISCO } from "@/lib/format";

const VARIANTE = {
  seguro: "success",
  moderado: "warning",
  avancado: "danger",
} as const;

export function RiskBadge({ risco }: { risco: Risco | null }) {
  if (!risco) {
    return <Badge variant="outline">Não catalogado</Badge>;
  }
  return <Badge variant={VARIANTE[risco]}>{ROTULO_RISCO[risco]}</Badge>;
}
