import {
  History,
  LayoutDashboard,
  ListChecks,
  SlidersHorizontal,
  Sparkles,
  Wrench,
} from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Toaster, toast } from "sonner";
import { ReportDialog, type RelatorioExibido } from "@/components/ReportDialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { api, mensagemDeErro, type ApplyReport, type ServiceDto } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Backups } from "@/pages/Backups";
import { Dashboard } from "@/pages/Dashboard";
import { Effects } from "@/pages/Effects";
import { Profiles } from "@/pages/Profiles";
import { Services } from "@/pages/Services";

type Pagina = "dashboard" | "servicos" | "efeitos" | "perfis" | "backups";

const NAVEGACAO: { id: Pagina; rotulo: string; icone: ReactNode }[] = [
  { id: "dashboard", rotulo: "Dashboard", icone: <LayoutDashboard className="size-4" /> },
  { id: "servicos", rotulo: "Serviços", icone: <ListChecks className="size-4" /> },
  { id: "efeitos", rotulo: "Efeitos visuais", icone: <Sparkles className="size-4" /> },
  { id: "perfis", rotulo: "Perfis", icone: <SlidersHorizontal className="size-4" /> },
  { id: "backups", rotulo: "Backups", icone: <History className="size-4" /> },
];

export default function App() {
  const [pagina, setPagina] = useState<Pagina>("dashboard");
  const [servicos, setServicos] = useState<ServiceDto[]>([]);
  const [carregando, setCarregando] = useState(true);
  const [relatorio, setRelatorio] = useState<RelatorioExibido | null>(null);

  const buscarServicos = useCallback(async () => {
    try {
      setServicos(await api.listServices());
    } catch (erro) {
      toast.error("Não foi possível listar os serviços", {
        description: mensagemDeErro(erro),
        duration: 12000,
      });
    } finally {
      setCarregando(false);
    }
  }, []);

  const recarregar = useCallback(async () => {
    setCarregando(true);
    await buscarServicos();
  }, [buscarServicos]);

  useEffect(() => {
    let ativo = true;
    api
      .listServices()
      .then((lista) => ativo && setServicos(lista))
      .catch((erro: unknown) =>
        toast.error("Não foi possível listar os serviços", {
          description: mensagemDeErro(erro),
          duration: 12000,
        }),
      )
      .finally(() => ativo && setCarregando(false));
    return () => {
      ativo = false;
    };
  }, []);

  const mostrarRelatorio = useCallback((titulo: string, dados: ApplyReport) => {
    setRelatorio({ titulo, relatorio: dados });
  }, []);

  return (
    <TooltipProvider delayDuration={350}>
      <div className="flex h-full">
        <aside className="flex w-56 shrink-0 flex-col border-r bg-card/60 p-3">
          <div className="flex items-center gap-2.5 px-2 py-3">
            <div className="flex size-9 items-center justify-center rounded-lg bg-primary text-primary-foreground">
              <Wrench className="size-5" />
            </div>
            <div>
              <div className="leading-tight font-semibold">WinTune</div>
              <div className="text-xs text-muted-foreground">Serviços do Windows</div>
            </div>
          </div>

          <nav className="mt-3 flex flex-col gap-1">
            {NAVEGACAO.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => setPagina(item.id)}
                aria-current={pagina === item.id ? "page" : undefined}
                className={cn(
                  "flex items-center gap-2.5 rounded-md px-3 py-2 text-sm font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                  pagina === item.id
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
                )}
              >
                {item.icone}
                {item.rotulo}
              </button>
            ))}
          </nav>

          <p className="mt-auto px-2 text-xs leading-relaxed text-muted-foreground">
            Serviços essenciais do Windows ficam bloqueados e nunca são alterados.
          </p>
        </aside>

        <main className="min-w-0 flex-1 p-6">
          {pagina === "dashboard" && (
            <Dashboard
              servicos={servicos}
              carregando={carregando}
              recarregarServicos={recarregar}
              mostrarRelatorio={mostrarRelatorio}
            />
          )}
          {pagina === "servicos" && (
            <Services servicos={servicos} carregando={carregando} recarregar={recarregar} />
          )}
          {pagina === "efeitos" && <Effects mostrarRelatorio={mostrarRelatorio} />}
          {pagina === "perfis" && (
            <Profiles
              servicos={servicos}
              recarregarServicos={recarregar}
              mostrarRelatorio={mostrarRelatorio}
            />
          )}
          {pagina === "backups" && (
            <Backups recarregarServicos={recarregar} mostrarRelatorio={mostrarRelatorio} />
          )}
        </main>
      </div>

      <ReportDialog exibido={relatorio} onFechar={() => setRelatorio(null)} />
      <Toaster theme="dark" position="bottom-right" richColors closeButton />
    </TooltipProvider>
  );
}
