import { Loader2, Lock, RefreshCw, Search } from "lucide-react";
import { memo, useCallback, useMemo, useState } from "react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { RiskBadge } from "@/components/RiskBadge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input, Select } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Tooltip } from "@/components/ui/tooltip";
import { api, mensagemDeErro, type ServiceDto, type StartType } from "@/lib/api";
import {
  CATEGORIAS,
  ROTULO_START_TYPE,
  ROTULO_STATUS,
  START_TYPES_APLICAVEIS,
  estaAtivo,
  plural,
} from "@/lib/format";
import { cn } from "@/lib/utils";

interface ServicesProps {
  servicos: ServiceDto[];
  carregando: boolean;
  recarregar: () => Promise<void>;
}

type FiltroStatus = "todos" | "rodando" | "parados" | "desativados" | "alterados";
type FiltroRisco = "todos" | "seguro" | "moderado" | "avancado" | "nao_catalogado";

const SEM_CATEGORIA = "__sem_categoria__";

interface LinhaProps {
  servico: ServiceDto;
  ocupado: boolean;
  onAlternar: (servico: ServiceDto, ligar: boolean) => void;
  onStartType: (servico: ServiceDto, tipo: StartType) => void;
}

const Linha = memo(function Linha({ servico, ocupado, onAlternar, onStartType }: LinhaProps) {
  const ativo = estaAtivo(servico.status);
  const ligado = servico.start_type !== "disabled";
  // Serviços restritos (ex.: Windows Update) nunca podem ser desligados pelo toggle.
  const toggleBloqueado = servico.protegido || servico.restrito;

  const nome = (
    <div className="min-w-0">
      <div className="flex items-center gap-2">
        <span className="truncate font-medium">{servico.display_name}</span>
        {servico.alterado && <Badge variant="primary">Alterado</Badge>}
      </div>
      <div className="truncate font-mono text-xs text-muted-foreground">{servico.name}</div>
    </div>
  );

  return (
    <tr className={cn("border-b transition-colors hover:bg-accent/40", servico.protegido && "opacity-60")}>
      <td className="w-16 py-2 pl-4">
        {toggleBloqueado ? (
          <Tooltip conteudo={servico.motivo_protecao ?? "Serviço protegido."} lado="right">
            <span className="flex items-center gap-1.5">
              <Lock className="size-4 text-muted-foreground" aria-label="Serviço protegido" />
              <Switch checked={ligado} disabled aria-label={`${servico.display_name} (bloqueado)`} />
            </span>
          </Tooltip>
        ) : ocupado ? (
          <Loader2 className="ml-2 size-4 animate-spin text-muted-foreground" />
        ) : (
          <Switch
            checked={ligado}
            onCheckedChange={(ligar) => onAlternar(servico, ligar)}
            aria-label={`${ligado ? "Desativar" : "Ativar"} ${servico.display_name}`}
          />
        )}
      </td>
      <td className="max-w-0 py-2 pr-3">
        {servico.descricao ? <Tooltip conteudo={servico.descricao}>{nome}</Tooltip> : nome}
      </td>
      <td className="w-36 py-2 pr-3 text-xs text-muted-foreground">{servico.categoria ?? "—"}</td>
      <td className="w-32 py-2 pr-3">
        <RiskBadge risco={servico.risco} />
      </td>
      <td className="w-32 py-2 pr-3 text-xs">
        <span className="flex items-center gap-1.5">
          <span
            className={cn("size-2 rounded-full", ativo ? "bg-success" : "bg-muted-foreground/50")}
          />
          {ROTULO_STATUS[servico.status]}
        </span>
      </td>
      <td className="w-48 py-2 pr-4">
        {servico.protegido || servico.start_type === "unknown" ? (
          <span className="text-xs text-muted-foreground">
            {ROTULO_START_TYPE[servico.start_type]}
          </span>
        ) : (
          <Select
            className="h-8 w-full text-xs"
            value={servico.start_type}
            disabled={ocupado}
            aria-label={`Tipo de inicialização de ${servico.display_name}`}
            onChange={(e) => onStartType(servico, e.target.value as StartType)}
          >
            {START_TYPES_APLICAVEIS.map((tipo) => (
              <option key={tipo} value={tipo} disabled={servico.restrito && tipo === "disabled"}>
                {ROTULO_START_TYPE[tipo]}
              </option>
            ))}
          </Select>
        )}
      </td>
    </tr>
  );
});

export function Services({ servicos, carregando, recarregar }: ServicesProps) {
  const [busca, setBusca] = useState("");
  const [categoria, setCategoria] = useState("todas");
  const [risco, setRisco] = useState<FiltroRisco>("todos");
  const [status, setStatus] = useState<FiltroStatus>("todos");
  const [ocupados, setOcupados] = useState<ReadonlySet<string>>(new Set());
  const [aConfirmar, setAConfirmar] = useState<ServiceDto | null>(null);

  const executar = useCallback(
    async (servico: ServiceDto, acao: () => Promise<string | null>, sucesso: string) => {
      setOcupados((atual) => new Set(atual).add(servico.name));
      try {
        const aviso = await acao();
        if (aviso) {
          toast.warning(servico.display_name, { description: aviso, duration: 9000 });
        } else {
          toast.success(servico.display_name, { description: sucesso });
        }
      } catch (erro) {
        toast.error(`Não foi possível alterar ${servico.display_name}`, {
          description: mensagemDeErro(erro),
          duration: 9000,
        });
      } finally {
        await recarregar();
        setOcupados((atual) => {
          const proximo = new Set(atual);
          proximo.delete(servico.name);
          return proximo;
        });
      }
    },
    [recarregar],
  );

  const aplicarToggle = useCallback(
    (servico: ServiceDto, ligar: boolean) =>
      executar(
        servico,
        () => api.setServiceEnabled(servico.name, ligar),
        ligar ? "Serviço reativado." : "Serviço desativado e parado.",
      ),
    [executar],
  );

  const alternar = useCallback(
    (servico: ServiceDto, ligar: boolean) => {
      // Desativar algo fora do catálogo ou de risco avançado pede confirmação.
      if (!ligar && (servico.risco === null || servico.risco === "avancado")) {
        setAConfirmar(servico);
        return;
      }
      void aplicarToggle(servico, ligar);
    },
    [aplicarToggle],
  );

  const mudarStartType = useCallback(
    (servico: ServiceDto, tipo: StartType) => {
      if (tipo === servico.start_type) return;
      if (tipo === "disabled") {
        alternar(servico, false);
        return;
      }
      void executar(
        servico,
        () => api.setStartType(servico.name, tipo),
        `Inicialização definida como ${ROTULO_START_TYPE[tipo]}.`,
      );
    },
    [alternar, executar],
  );

  const filtrados = useMemo(() => {
    const termo = busca.trim().toLowerCase();
    return servicos
      .filter((s) => {
        if (
          termo &&
          !s.name.toLowerCase().includes(termo) &&
          !s.display_name.toLowerCase().includes(termo) &&
          !(s.descricao ?? "").toLowerCase().includes(termo)
        ) {
          return false;
        }
        if (categoria === SEM_CATEGORIA ? s.categoria !== null : categoria !== "todas" && s.categoria !== categoria) {
          return false;
        }
        if (risco === "nao_catalogado" ? s.risco !== null : risco !== "todos" && s.risco !== risco) {
          return false;
        }
        switch (status) {
          case "rodando":
            return estaAtivo(s.status);
          case "parados":
            return !estaAtivo(s.status);
          case "desativados":
            return s.start_type === "disabled";
          case "alterados":
            return s.alterado;
          default:
            return true;
        }
      })
      .sort(
        (a, b) =>
          Number(b.categoria !== null) - Number(a.categoria !== null) ||
          a.display_name.localeCompare(b.display_name, "pt-BR"),
      );
  }, [servicos, busca, categoria, risco, status]);

  return (
    <div className="flex h-full flex-col gap-4">
      <header className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">Serviços</h1>
          <p className="text-sm text-muted-foreground">
            {plural(filtrados.length, "serviço", "serviços")} de {servicos.length}. Passe o mouse
            sobre o nome para ver o que cada um faz.
          </p>
        </div>
        <Button variant="outline" onClick={() => void recarregar()} disabled={carregando}>
          <RefreshCw className={cn(carregando && "animate-spin")} />
          Atualizar
        </Button>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-56 flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-9"
            placeholder="Buscar por nome ou descrição…"
            value={busca}
            onChange={(e) => setBusca(e.target.value)}
          />
        </div>
        <Select value={categoria} onChange={(e) => setCategoria(e.target.value)} aria-label="Categoria">
          <option value="todas">Todas as categorias</option>
          {CATEGORIAS.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
          <option value={SEM_CATEGORIA}>Sem categoria</option>
        </Select>
        <Select value={risco} onChange={(e) => setRisco(e.target.value as FiltroRisco)} aria-label="Risco">
          <option value="todos">Todos os riscos</option>
          <option value="seguro">Seguro</option>
          <option value="moderado">Moderado</option>
          <option value="avancado">Avançado</option>
          <option value="nao_catalogado">Não catalogado</option>
        </Select>
        <Select value={status} onChange={(e) => setStatus(e.target.value as FiltroStatus)} aria-label="Status">
          <option value="todos">Todos os status</option>
          <option value="rodando">Em execução</option>
          <option value="parados">Parados</option>
          <option value="desativados">Desativados</option>
          <option value="alterados">Alterados pelo app</option>
        </Select>
      </div>

      <div className="min-h-0 flex-1 overflow-auto rounded-xl border bg-card">
        <table className="w-full table-fixed text-sm">
          <thead className="sticky top-0 z-10 bg-card text-left text-xs text-muted-foreground shadow-[0_1px_0_var(--border)]">
            <tr>
              <th className="w-16 py-2.5 pl-4 font-medium">Ativo</th>
              <th className="py-2.5 pr-3 font-medium">Serviço</th>
              <th className="w-36 py-2.5 pr-3 font-medium">Categoria</th>
              <th className="w-32 py-2.5 pr-3 font-medium">Risco</th>
              <th className="w-32 py-2.5 pr-3 font-medium">Status</th>
              <th className="w-48 py-2.5 pr-4 font-medium">Inicialização</th>
            </tr>
          </thead>
          <tbody>
            {filtrados.map((servico) => (
              <Linha
                key={servico.name}
                servico={servico}
                ocupado={ocupados.has(servico.name)}
                onAlternar={alternar}
                onStartType={mudarStartType}
              />
            ))}
          </tbody>
        </table>
        {filtrados.length === 0 && (
          <p className="py-16 text-center text-sm text-muted-foreground">
            {carregando ? "Carregando serviços…" : "Nenhum serviço corresponde aos filtros."}
          </p>
        )}
      </div>

      <ConfirmDialog
        aberto={aConfirmar !== null}
        perigoso
        titulo={`Desativar ${aConfirmar?.display_name ?? ""}?`}
        rotuloConfirmar="Desativar mesmo assim"
        descricao={
          aConfirmar?.risco === "avancado" ? (
            <>
              <p className="font-medium text-danger">Risco avançado.</p>
              <p className="mt-1">{aConfirmar.descricao}</p>
            </>
          ) : (
            <p>
              Este serviço não está no catálogo do WinTune, então não sabemos o que pode parar de
              funcionar. Você poderá reativá-lo aqui ou em Backups → Restaurar original.
            </p>
          )
        }
        onCancelar={() => setAConfirmar(null)}
        onConfirmar={() => {
          if (aConfirmar) void aplicarToggle(aConfirmar, false);
          setAConfirmar(null);
        }}
      />
    </div>
  );
}
