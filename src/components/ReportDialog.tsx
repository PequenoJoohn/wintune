import { AlertTriangle, CheckCircle2, MinusCircle, XCircle } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { ApplyReport, ItemRelatorio } from "@/lib/api";
import { plural } from "@/lib/format";

export interface RelatorioExibido {
  titulo: string;
  relatorio: ApplyReport;
}

interface SecaoProps {
  titulo: string;
  icone: ReactNode;
  itens: ItemRelatorio[];
  aberta?: boolean;
}

function Secao({ titulo, icone, itens, aberta = false }: SecaoProps) {
  if (itens.length === 0) return null;
  return (
    <details open={aberta} className="group rounded-lg border bg-card/50">
      <summary className="flex cursor-pointer items-center gap-2 px-3 py-2 text-sm font-medium">
        {icone}
        {titulo} ({itens.length})
      </summary>
      <ul className="divide-y border-t text-sm">
        {itens.map((item) => (
          <li key={item.name} className="px-3 py-2">
            <div className="flex items-baseline justify-between gap-3">
              <span className="font-medium">{item.display_name}</span>
              <span className="shrink-0 font-mono text-xs text-muted-foreground">{item.name}</span>
            </div>
            <p className="select-text text-xs text-muted-foreground">{item.detalhe}</p>
            {item.aviso && (
              <p className="mt-1 flex select-text items-start gap-1 text-xs text-warning">
                <AlertTriangle className="mt-0.5 size-3 shrink-0" />
                {item.aviso}
              </p>
            )}
          </li>
        ))}
      </ul>
    </details>
  );
}

interface ReportDialogProps {
  exibido: RelatorioExibido | null;
  onFechar: () => void;
}

/** Relatório após uma operação em lote: X alterados, Y falharam (com motivo), Z ignorados. */
export function ReportDialog({ exibido, onFechar }: ReportDialogProps) {
  const relatorio = exibido?.relatorio;
  return (
    <Dialog open={exibido !== null} onOpenChange={(abrir) => !abrir && onFechar()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{exibido?.titulo}</DialogTitle>
          <DialogDescription>
            {relatorio &&
              `${plural(relatorio.sucessos.length, "alterado", "alterados")}, ${plural(
                relatorio.falhas.length,
                "falhou",
                "falharam",
              )}, ${plural(relatorio.ignorados.length, "ignorado", "ignorados")}.`}
            {relatorio?.snapshot_id &&
              " Um backup automático foi criado antes das alterações (veja em Backups)."}
          </DialogDescription>
        </DialogHeader>

        {relatorio && (
          <div className="flex min-h-0 flex-col gap-2 overflow-y-auto pr-1">
            <Secao
              titulo="Falharam"
              icone={<XCircle className="size-4 text-danger" />}
              itens={relatorio.falhas}
              aberta
            />
            <Secao
              titulo="Alterados"
              icone={<CheckCircle2 className="size-4 text-success" />}
              itens={relatorio.sucessos}
              aberta={relatorio.falhas.length === 0}
            />
            <Secao
              titulo="Ignorados"
              icone={<MinusCircle className="size-4 text-muted-foreground" />}
              itens={relatorio.ignorados}
            />
            {relatorio.sucessos.length + relatorio.falhas.length + relatorio.ignorados.length ===
              0 && (
              <p className="rounded-lg border bg-card/50 px-3 py-4 text-center text-sm text-muted-foreground">
                Nada precisou ser alterado: o sistema já estava nesse estado.
              </p>
            )}
          </div>
        )}

        <DialogFooter>
          <Button onClick={onFechar}>Fechar</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
