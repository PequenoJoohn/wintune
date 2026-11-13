import { History, Loader2, Plus, RotateCcw, ShieldCheck, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api, mensagemDeErro, type ApplyReport, type SnapshotInfo } from "@/lib/api";
import { ROTULO_SNAPSHOT, formatarData } from "@/lib/format";

interface BackupsProps {
  recarregarServicos: () => Promise<void>;
  mostrarRelatorio: (titulo: string, relatorio: ApplyReport) => void;
}

const VARIANTE_TIPO = { original: "primary", automatico: "default", manual: "success" } as const;

export function Backups({ recarregarServicos, mostrarRelatorio }: BackupsProps) {
  const [snapshots, setSnapshots] = useState<SnapshotInfo[]>([]);
  const [carregando, setCarregando] = useState(true);
  const [acao, setAcao] = useState<"criar" | "ponto" | "restaurar" | "excluir" | null>(null);
  const [aRestaurar, setARestaurar] = useState<SnapshotInfo | null>(null);
  const [aExcluir, setAExcluir] = useState<SnapshotInfo | null>(null);

  const carregar = useCallback(async () => {
    try {
      setSnapshots(await api.listSnapshots());
    } catch (erro) {
      toast.error("Não foi possível listar os backups", { description: mensagemDeErro(erro) });
    } finally {
      setCarregando(false);
    }
  }, []);

  useEffect(() => {
    let ativo = true;
    api
      .listSnapshots()
      .then((lista) => ativo && setSnapshots(lista))
      .catch((erro: unknown) =>
        toast.error("Não foi possível listar os backups", { description: mensagemDeErro(erro) }),
      )
      .finally(() => ativo && setCarregando(false));
    return () => {
      ativo = false;
    };
  }, []);

  async function criarBackup() {
    setAcao("criar");
    try {
      const info = await api.createSnapshot();
      toast.success("Backup criado", {
        description: `${info.total_servicos} serviços${info.total_efeitos > 0 ? ` e ${info.total_efeitos} efeitos visuais` : ""} salvos em ${formatarData(info.criado_em)}.`,
      });
      await carregar();
    } catch (erro) {
      toast.error("Não foi possível criar o backup", { description: mensagemDeErro(erro) });
    } finally {
      setAcao(null);
    }
  }

  async function criarPontoDeRestauracao() {
    setAcao("ponto");
    try {
      const resultado = await api.createRestorePoint();
      if (resultado.status === "criado") {
        toast.success("Ponto de restauração criado", { description: resultado.mensagem });
      } else {
        toast.info("Ponto de restauração recente", {
          description: resultado.mensagem,
          duration: 10000,
        });
      }
    } catch (erro) {
      toast.error("Ponto de restauração não criado", {
        description: mensagemDeErro(erro),
        duration: 12000,
      });
    } finally {
      setAcao(null);
    }
  }

  async function restaurar(snapshot: SnapshotInfo) {
    setAcao("restaurar");
    try {
      const relatorio = await api.restoreSnapshot(snapshot.id);
      mostrarRelatorio(`Backup de ${formatarData(snapshot.criado_em)} restaurado`, relatorio);
    } catch (erro) {
      toast.error("Não foi possível restaurar o backup", { description: mensagemDeErro(erro) });
    } finally {
      setARestaurar(null);
      setAcao(null);
      await Promise.all([carregar(), recarregarServicos()]);
    }
  }

  async function excluir(snapshot: SnapshotInfo) {
    setAcao("excluir");
    try {
      await api.deleteSnapshot(snapshot.id);
      toast.success("Backup excluído");
    } catch (erro) {
      toast.error("Não foi possível excluir o backup", { description: mensagemDeErro(erro) });
    } finally {
      setAExcluir(null);
      setAcao(null);
      await carregar();
    }
  }

  return (
    <div className="flex h-full flex-col gap-4">
      <header className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">Backups</h1>
          <p className="text-sm text-muted-foreground">
            Cada backup guarda o tipo de inicialização e o estado de todos os serviços, além dos
            efeitos visuais. Um backup automático é criado antes de qualquer alteração em lote.
          </p>
        </div>
        <div className="flex shrink-0 gap-2">
          <Button
            variant="outline"
            onClick={() => void criarPontoDeRestauracao()}
            disabled={acao !== null}
            title="Cria um ponto de restauração do Windows (Checkpoint-Computer)"
          >
            {acao === "ponto" ? <Loader2 className="animate-spin" /> : <ShieldCheck />}
            Ponto de restauração do Windows
          </Button>
          <Button onClick={() => void criarBackup()} disabled={acao !== null}>
            {acao === "criar" ? <Loader2 className="animate-spin" /> : <Plus />}
            Criar backup agora
          </Button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto pr-1">
        {snapshots.map((snapshot) => (
          <Card key={snapshot.id} className="flex items-center gap-4 px-5 py-3.5">
            <History className="size-5 shrink-0 text-muted-foreground" />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="font-medium">{formatarData(snapshot.criado_em)}</span>
                <Badge variant={VARIANTE_TIPO[snapshot.tipo]}>{ROTULO_SNAPSHOT[snapshot.tipo]}</Badge>
              </div>
              <p className="truncate text-sm text-muted-foreground">
                {snapshot.motivo} · {snapshot.total_servicos} serviços
                {snapshot.total_efeitos > 0 && ` · ${snapshot.total_efeitos} efeitos visuais`}
              </p>
            </div>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setARestaurar(snapshot)}
              disabled={acao !== null}
            >
              <RotateCcw />
              Restaurar
            </Button>
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setAExcluir(snapshot)}
              disabled={acao !== null || snapshot.tipo === "original"}
              title={
                snapshot.tipo === "original"
                  ? "O backup original nunca pode ser excluído"
                  : "Excluir backup"
              }
              aria-label="Excluir backup"
            >
              <Trash2 />
            </Button>
          </Card>
        ))}
        {snapshots.length === 0 && (
          <p className="py-16 text-center text-sm text-muted-foreground">
            {carregando ? "Carregando backups…" : "Nenhum backup encontrado."}
          </p>
        )}
      </div>

      <ConfirmDialog
        aberto={aRestaurar !== null}
        ocupado={acao === "restaurar"}
        titulo="Restaurar este backup?"
        rotuloConfirmar="Restaurar"
        descricao={
          <p>
            Todos os serviços que mudaram desde {aRestaurar && formatarData(aRestaurar.criado_em)}{" "}
            voltarão ao tipo de inicialização daquele momento. Um backup automático do estado atual
            será criado antes.
          </p>
        }
        onCancelar={() => setARestaurar(null)}
        onConfirmar={() => aRestaurar && void restaurar(aRestaurar)}
      />

      <ConfirmDialog
        aberto={aExcluir !== null}
        ocupado={acao === "excluir"}
        perigoso
        titulo="Excluir este backup?"
        rotuloConfirmar="Excluir"
        descricao={
          <p>
            O backup de {aExcluir && formatarData(aExcluir.criado_em)} será apagado do disco. Essa
            ação não pode ser desfeita.
          </p>
        }
        onCancelar={() => setAExcluir(null)}
        onConfirmar={() => aExcluir && void excluir(aExcluir)}
      />
    </div>
  );
}
