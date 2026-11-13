import { Activity, Layers, MemoryStick, PowerOff, RotateCcw, ShieldAlert } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  ID_ORIGINAL,
  api,
  mensagemDeErro,
  type ApplyReport,
  type ServiceDto,
  type SystemStats,
} from "@/lib/api";
import { estaAtivo, formatarBytes, plural } from "@/lib/format";
import { cn } from "@/lib/utils";

interface DashboardProps {
  servicos: ServiceDto[];
  carregando: boolean;
  recarregarServicos: () => Promise<void>;
  mostrarRelatorio: (titulo: string, relatorio: ApplyReport) => void;
}

const INTERVALO_STATS_MS = 3000;

function Indicador({
  icone,
  rotulo,
  valor,
  detalhe,
}: {
  icone: ReactNode;
  rotulo: string;
  valor: string;
  detalhe?: ReactNode;
}) {
  return (
    <Card>
      <CardContent className="flex flex-col gap-2 p-5">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          {icone}
          {rotulo}
        </div>
        <div className="text-3xl font-semibold tabular-nums">{valor}</div>
        {detalhe && <div className="text-xs text-muted-foreground">{detalhe}</div>}
      </CardContent>
    </Card>
  );
}

export function Dashboard({
  servicos,
  carregando,
  recarregarServicos,
  mostrarRelatorio,
}: DashboardProps) {
  const [stats, setStats] = useState<SystemStats | null>(null);
  const [confirmando, setConfirmando] = useState(false);
  const [restaurando, setRestaurando] = useState(false);

  useEffect(() => {
    let ativo = true;
    const atualizar = () => {
      api
        .systemStats()
        .then((s) => ativo && setStats(s))
        .catch(() => undefined);
    };
    atualizar();
    const temporizador = window.setInterval(atualizar, INTERVALO_STATS_MS);
    return () => {
      ativo = false;
      window.clearInterval(temporizador);
    };
  }, []);

  const resumo = useMemo(
    () => ({
      total: servicos.length,
      rodando: servicos.filter((s) => estaAtivo(s.status)).length,
      desativadosPeloApp: servicos.filter((s) => s.alterado && s.start_type === "disabled").length,
      alterados: servicos.filter((s) => s.alterado).length,
    }),
    [servicos],
  );

  const usoRam = stats && stats.ram_total > 0 ? stats.ram_usada / stats.ram_total : 0;

  async function restaurarOriginal() {
    setRestaurando(true);
    try {
      const relatorio = await api.restoreSnapshot(ID_ORIGINAL);
      mostrarRelatorio("Estado original restaurado", relatorio);
    } catch (erro) {
      toast.error("Não foi possível restaurar o estado original", {
        description: mensagemDeErro(erro),
        duration: 12000,
      });
    } finally {
      setConfirmando(false);
      setRestaurando(false);
      await recarregarServicos();
    }
  }

  const valor = (n: number) => (carregando && servicos.length === 0 ? "—" : String(n));

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto pr-1">
      <header>
        <h1 className="text-2xl font-semibold">Dashboard</h1>
        <p className="text-sm text-muted-foreground">Visão geral dos serviços deste computador.</p>
      </header>

      {stats && !stats.elevado && (
        <div className="flex items-start gap-3 rounded-xl border border-warning/40 bg-warning/10 p-4 text-sm">
          <ShieldAlert className="mt-0.5 size-5 shrink-0 text-warning" />
          <div>
            <p className="font-medium text-warning">O WinTune não está rodando como administrador</p>
            <p className="text-muted-foreground">
              Os serviços aparecem bloqueados porque o Windows nega alterações sem elevação. Feche e
              abra o app como administrador.
            </p>
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <Indicador
          icone={<Layers className="size-4" />}
          rotulo="Total de serviços"
          valor={valor(resumo.total)}
        />
        <Indicador
          icone={<Activity className="size-4 text-success" />}
          rotulo="Em execução"
          valor={valor(resumo.rodando)}
          detalhe={`${valor(resumo.total - resumo.rodando)} parados`}
        />
        <Indicador
          icone={<PowerOff className="size-4 text-primary" />}
          rotulo="Desativados pelo WinTune"
          valor={valor(resumo.desativadosPeloApp)}
          detalhe={`${plural(resumo.alterados, "serviço difere", "serviços diferem")} do original`}
        />
        <Indicador
          icone={<MemoryStick className="size-4 text-warning" />}
          rotulo="Uso de RAM"
          valor={stats ? `${Math.round(usoRam * 100)}%` : "—"}
          detalhe={
            stats && (
              <div className="flex flex-col gap-1.5">
                <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                  <div
                    className={cn(
                      "h-full rounded-full transition-all duration-500",
                      usoRam > 0.85 ? "bg-danger" : usoRam > 0.65 ? "bg-warning" : "bg-success",
                    )}
                    style={{ width: `${Math.round(usoRam * 100)}%` }}
                  />
                </div>
                {formatarBytes(stats.ram_usada)} de {formatarBytes(stats.ram_total)}
              </div>
            )
          }
        />
      </div>

      <Card className="mt-auto">
        <CardContent className="flex items-center justify-between gap-6 p-6">
          <div>
            <h2 className="text-lg font-semibold">Algo parou de funcionar?</h2>
            <p className="max-w-xl text-sm text-muted-foreground">
              Volte todos os serviços e efeitos visuais ao estado em que estavam na primeira vez
              que o WinTune foi aberto. O backup original nunca é sobrescrito.
            </p>
          </div>
          <Button
            size="lg"
            className="shrink-0"
            onClick={() => setConfirmando(true)}
            disabled={restaurando}
          >
            <RotateCcw className="size-5!" />
            Restaurar original
          </Button>
        </CardContent>
      </Card>

      <ConfirmDialog
        aberto={confirmando}
        ocupado={restaurando}
        titulo="Restaurar o estado original?"
        rotuloConfirmar="Restaurar original"
        descricao={
          <p>
            {resumo.alterados > 0
              ? `${plural(resumo.alterados, "serviço voltará", "serviços voltarão")} ao tipo de inicialização original, e os que estavam em execução serão iniciados.`
              : "Nenhum serviço difere do estado original no momento."}{" "}
            Os efeitos visuais também voltam ao original. Um backup automático do estado atual será
            criado antes.
          </p>
        }
        onCancelar={() => setConfirmando(false)}
        onConfirmar={() => void restaurarOriginal()}
      />
    </div>
  );
}
