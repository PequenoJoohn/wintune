import { Loader2 } from "lucide-react";
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

interface ConfirmDialogProps {
  aberto: boolean;
  titulo: string;
  descricao: ReactNode;
  rotuloConfirmar: string;
  perigoso?: boolean;
  ocupado?: boolean;
  confirmarDesabilitado?: boolean;
  children?: ReactNode;
  className?: string;
  onConfirmar: () => void;
  onCancelar: () => void;
}

export function ConfirmDialog({
  aberto,
  titulo,
  descricao,
  rotuloConfirmar,
  perigoso = false,
  ocupado = false,
  confirmarDesabilitado = false,
  children,
  className,
  onConfirmar,
  onCancelar,
}: ConfirmDialogProps) {
  return (
    <Dialog open={aberto} onOpenChange={(abrir) => !abrir && !ocupado && onCancelar()}>
      <DialogContent travado={ocupado} className={className}>
        <DialogHeader>
          <DialogTitle>{titulo}</DialogTitle>
          <DialogDescription asChild>
            <div>{descricao}</div>
          </DialogDescription>
        </DialogHeader>
        {children}
        <DialogFooter>
          <Button variant="outline" onClick={onCancelar} disabled={ocupado}>
            Cancelar
          </Button>
          <Button
            variant={perigoso ? "destructive" : "default"}
            onClick={onConfirmar}
            disabled={ocupado || confirmarDesabilitado}
          >
            {ocupado && <Loader2 className="animate-spin" />}
            {rotuloConfirmar}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
