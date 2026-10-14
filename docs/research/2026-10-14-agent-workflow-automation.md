# Automazione del processo per gli agenti in signaltty

Ricerca del 14 ottobre 2026. La prima raccomandazione è chiarire dove vive ogni informazione e quale fonte decide in caso di conflitto. Su questa organizzazione si possono rendere interrogabili stato, contesto e verifiche usando gli script esistenti. Ogni nuovo obbligo di lettura o passaggio di revisione deve risolvere un errore osservato e superare una valutazione su task reali.

Questa nota distingue risultati sperimentali, esperienze dei fornitori e proposte per signaltty. Le fonti online sono state aperte durante la ricerca. La [nota del 3 ottobre](2026-10-03-agent-ready-repository.md) è contesto storico, non prova delle affermazioni qui riportate. Non sono stati eseguiti test del prodotto.

## Le fonti non dimostrano che più istruzioni migliorino il risultato

*Evaluating AGENTS.md*, versione 3, non trova differenze statisticamente significative nella risoluzione dei task rispetto all'assenza di file di contesto. I file generati da LLM aumentano il costo medio del 20% su SWE-bench e del 23% su CTXbench. Quelli scritti dagli sviluppatori migliorano marginalmente la risoluzione, senza significatività statistica, e aumentano il costo fino al 19%. Gli autori raccomandano istruzioni aggiuntive specifiche, evitando informazioni già recuperabili dal repo. Lo studio usa task Python e agenti determinati: non certifica un processo Rust e GTK. [Gloaguen et al., v3](https://arxiv.org/html/2602.11988v3).

Un altro studio analizza 10 repository e 124 pull request. Riporta una riduzione del runtime mediano del 28,64% e dei token di output del 16,58% con `AGENTS.md`, mantenendo un comportamento di completamento comparabile. Campione, agenti e metriche sono diversi. I token di output non equivalgono al costo totale di inferenza, né il runtime prova l'assenza di regressioni. Questi risultati impediscono di sostenere un consenso generale sull'inutilità o sull'efficacia di `AGENTS.md`. [Lulla et al., v2](https://arxiv.org/html/2601.20404v2).

La conclusione per signaltty è una proposta: conservare comandi affidabili e vincoli non ovvi, poi misurare l'effetto delle istruzioni sul progetto.

## Le esperienze utili dei fornitori

OpenAI descrive un `AGENTS.md` breve che indica dove trovare le informazioni, documentazione nel repository, istanze dell'app e osservabilità isolate per worktree, e controlli automatici dei confini architetturali. È un resoconto interno, non un esperimento controllato che garantisca risultati trasferibili. [OpenAI, Harness engineering, 11 febbraio 2026](https://openai.com/index/harness-engineering/).

Le indicazioni più recenti di OpenAI raccomandano descrizioni delle skill corte e precise, istruzioni mirate e revisione delle regole accumulate quando cambia il modello. Troppe descrizioni possono essere accorciate, rendendo meno chiara la scelta della skill. Questa indicazione riguarda Codex e un modello specifico; l'effetto su altri agenti va misurato. [OpenAI Developers, Rethinking skills and prompts for GPT-6 Astra, 11 settembre 2026](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra).

Anthropic descrive un ambiente iniziale eseguibile, una lista di requisiti, progressi persistenti e storia Git per riprendere il lavoro fra sessioni. L'agente implementa una parte del lavoro e verifica il comportamento nell'applicazione prima di dichiararlo completo. Sono esperienze su applicazioni web, non una prescrizione di duplicare `specs/` con un secondo registro. [Anthropic, Effective harnesses for long-running agents, 26 novembre 2025](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents).

Il successivo resoconto Anthropic propone di rimuovere un componente alla volta e verificare l'effetto sul risultato. Il valutatore aggiunge valore sui task che il modello non gestisce affidabilmente da solo, ma può approvare difetti o provare solo casi superficiali. L'autovalutazione e una seconda revisione LLM restano fallibili. [Anthropic, Harness design for long-running application development, 24 marzo 2026](https://www.anthropic.com/engineering/harness-design-long-running-apps).

Per gli strumenti, Anthropic raccomanda finalità distinte, risposte pertinenti e limitate, parametri chiari ed errori che spieghino la correzione necessaria. Suggerisce di valutare gli strumenti su task realistici senza imporre una sola sequenza valida di chiamate. È esperienza del fornitore; JSON non è dimostrato come formato universalmente migliore. [Anthropic, Writing effective tools for agents, 11 settembre 2025](https://www.anthropic.com/engineering/writing-tools-for-agents).

GitHub permette di preparare l'ambiente Copilot con `.github/workflows/copilot-setup-steps.yml`, un job `copilot-setup-steps` presente sul branch predefinito. Un errore nel setup non impedisce all'agente di partire nell'ambiente parzialmente preparato. Perciò un controllo esplicito dei prerequisiti conserva valore anche con quel workflow. [GitHub, Configure the development environment](https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/customize-cloud-agent/customize-the-agent-environment).

Il supporto delle istruzioni varia fra client e funzioni Copilot. Non basta aggiungere un file per presumere che ogni agente lo abbia letto. [GitHub, Support for different types of custom instructions](https://docs.github.com/en/copilot/reference/custom-instructions-support).

Anthropic distingue il resoconto dell'agente dal risultato nello stato finale dell'ambiente. Propone prove ripetute e verificatori scelti per il comportamento da valutare. I giudizi LLM richiedono calibrazione umana; una verifica tramite codice prova solo le condizioni che controlla. [Anthropic, Demystifying evals for AI agents, 9 gennaio 2026](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents).

## La base già presente in signaltty

L'ispezione di [docs/15](../15-agent-skills.md), [docs/17](../17-agent-development.md), della [costituzione](../../.specify/memory/constitution.md) e di [verify.py](../../scripts/verify.py) mostra queste basi:

- Setup ripetibile, toolchain fissata, container e doctor separato dall'installazione.
- Routing delle skill per task e processo `spec.md`, `plan.md`, `tasks.md`, con eccezioni documentate.
- Runner comune locale e CI, controlli architetturali e QA del server e di GTK in ambienti isolati.
- Directory di evidenza nuove a ogni esecuzione. `summary.json` conserva revisione, stato dirty, comandi, versioni, durate e risultati.
- Regole per worktree e ownership dei file, formato di handoff e task bank per valutare il processo.
- Limiti dichiarati: screenshot da ispezionare e comportamento nativo o manuale non certificato dai soli test Xvfb.

Gli script esaminati non espongono un comando comune `status`, `context`, `next`, `check`. Il runner registra se il checkout è dirty, ma non identifica il contenuto delle modifiche verificate. Il formato di handoff è una convenzione testuale. Sono le lacune su cui concentrerei il primo intervento.

## Esempi GitHub con uso degli agenti documentato

La selezione segue prove pubbliche di contributi o workflow agentici. Non è una classifica della quota di codice scritto da agenti: i repository non offrono una misura confrontabile di quel dato.

### GitHub Spec Kit

Spec Kit documenta il proprio sviluppo agentico. Le richieste possono passare da valutazione a lavoro SDD; i bug hanno workflow separati di diagnosi, correzione e verifica. Test, lint e release continuano a usare automazione deterministica. I passaggi controllati dai maintainer sono scelte del progetto pubblico, non obblighi da trasferire al lavoro locale autorizzato. La guida mostra adozione, non risparmi misurati. [Spec Kit, Agentic SDLC](https://github.github.com/spec-kit/guides/agentic-sdlc.html).

I file di workflow sono verificabili nel repository: [bug-assess.md](https://github.com/github/spec-kit/blob/main/.github/workflows/bug-assess.md), [bug-fix.md](https://github.com/github/spec-kit/blob/main/.github/workflows/bug-fix.md) e [bug-test.md](https://github.com/github/spec-kit/blob/main/.github/workflows/bug-test.md). I sorgenti Markdown e le rispettive versioni compilate `.lock.yml` hanno ruoli diversi. Per signaltty adotterei la distinzione esplicita fra sorgente, file generato e risultato del lavoro, oltre a percorsi diversi per indagine, feature e manutenzione entro le eccezioni della costituzione.

### Goose

Goose ha un [workflow per risolvere issue](https://github.com/block/goose/blob/main/.github/workflows/goose-issue-solver.yml) e uno per [rivedere PR](https://github.com/block/goose/blob/main/.github/workflows/goose-pr-reviewer.yml). Il primo prepara il contesto, esegue l'agente e propone una draft PR; il secondo chiede review senza modifiche al codice. I job hanno timeout di 30 e 15 minuti. Il limite di tre tentativi nel solver è un'istruzione nel prompt, non un contatore imposto dallo script. Questi file documentano automazione agentica disponibile, senza misurare quante volte venga usata.

Il [CLAUDE.md](https://github.com/block/goose/blob/main/CLAUDE.md) contiene soltanto un riferimento ad `AGENTS.md`; la [guida per la documentazione](https://github.com/block/goose/blob/main/documentation/AGENTS.md) aggiunge regole locali. L'[AGENTS.md principale](https://github.com/block/goose/blob/main/AGENTS.md) distingue l'issue pronta per una PR upstream dal lavoro locale o esplorativo. Per signaltty prenderei fonti uniche, istruzioni per area e workflow con responsabilità distinte. I vincoli di verifica del nostro progetto restano quelli della costituzione.

### OpenHands

Il repository attuale [OpenHands/OpenHands](https://github.com/OpenHands/OpenHands) contiene Agent Canvas. Il suo [AGENTS.md](https://github.com/OpenHands/OpenHands/blob/main/AGENTS.md) distingue skill pubbliche e guide per contribuire al progetto, elenca il repository responsabile di ogni area e rimanda alle skill pertinenti. Le regole dettagliate rimangono nei rispettivi `references/guide.md`. Mantiene stabili gli identificativi delle spec e marca le deprecazioni senza rinumerarle. Sono scelte osservabili di organizzazione, non prove della loro efficacia.

L'uso degli agenti è documentato oltre alla presenza di istruzioni: la [PR 18422](https://github.com/OpenHands/OpenHands/pull/18422), integrata il 14 ottobre 2026, dichiara un bug trovato durante una prova agentica completa e attribuisce preparazione del fix e delle evidenze a un agente. Riporta anche limiti delle prove e revisione del codice verificata. Questo caso dimostra un contributo assistito, ma non misura la quota del progetto sviluppata dagli agenti.

Per signaltty prenderei l'ownership esplicita e le fonti canoniche per area. Non copierei automaticamente la lunghezza delle istruzioni né ulteriori obblighi di approvazione.

### Codex: comandi e istruzioni vicino al codice

Il [justfile di Codex](https://github.com/openai/codex/blob/main/justfile) centralizza formattazione, test e rigenerazione degli schemi. Una [guida locale per il bottom pane](https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/AGENTS.md) colloca le istruzioni accanto all'area interessata. Il file `AGENTS.md` alla radice restituisce 404 nella ricerca corrente, benché sia ancora nei risultati del motore di ricerca: le copie indicizzate non provano la struttura attuale. Per signaltty è utile il modello di comandi stabili e guide locali; non occorre adottare `just` per ottenere lo stesso risultato con gli script già presenti.

## I punti di disordine osservabili nel checkout

L'inventario del 14 ottobre trova 70 directory sotto `specs/`, 37 voci sotto `docs/adr/` e 42 sotto `docs/qa/`. Sette prefissi delle spec ricorrono più volte: `023`, `024`, `025`, `026`, `041`, `045` e `057`. I nomi completi distinguono le feature, ma il solo numero non le identifica. Un indice dovrebbe usare il nome completo e rendere visibili stato, sostituzioni e riferimenti senza rinumerare la storia.

La fonte della skill di verifica è già unica in `.agents/skills/verify-signaltty`; Claude, Cursor e Copilot usano symlink. Questo assetto va conservato. `.specify/feature.json` seleziona una feature per gli strumenti, ma non registra lo stato generale di tutte le spec. Il routing in `docs/15` e l'indice `docs/README.md` possono guidare meglio la lettura distinguendo istruzioni vigenti, ricerca e prove storiche. La riga di `AGENTS.md` sul warning Clippy preesistente va riallineata alla baseline vuota e a `docs/17`.

## Le priorità proposte

Queste scelte sono proposte progettuali per signaltty. Non sono risultati sperimentali delle fonti.

Prima dell'automazione serve un riordino logico. Gli indici devono distinguere architettura vigente, ricerca, decisioni e prove storiche. Le spec devono avere stato attivo, completato o superato riconoscibile, conservando identità e riferimenti. Va dichiarato quali skill e comandi sono sorgenti canonici, quali sono upstream fissati e quali sono generati. È preferibile rendere leggibile la struttura esistente prima di spostare file o aggiungere cartelle. I crate applicativi non richiedono un riassetto generale per risolvere questo problema.

1. **Aggiungere un punto di ingresso sottile.** `scripts/agent.sh status --json` raccoglie feature selezionata, branch, worktree, fase osservata, task aperti e ultima verifica. `context --task ipc --json` restituisce documenti, skill e comandi pertinenti. `next --json` indica il prossimo passo consentito o un prerequisito mancante. `check --phase delivery` richiama le verifiche esistenti. Gli script spec-kit e `verify.sh` restano responsabili delle proprie operazioni. Il comando non deve inferire il completamento dal solo fatto che esista un file.

2. **Mantenere una sola fonte per task e regole.** Prima di introdurre un nuovo `state.json`, scegliere se estendere `tasks.md` e i metadati spec-kit oppure generare uno stato derivato. Il handoff aggiunge decisioni, ownership e prossimo passo, senza duplicare lo stato dei task. Una configurazione di routing può produrre sia `context` sia la tabella in `docs/15`, purché sostituisca la manutenzione manuale duplicata.

3. **Legare le prove al contenuto verificato.** Estendere l'evidenza con l'identità dei file rilevanti, incluse modifiche staged, unstaged e file nuovi, e verificare che quel contenuto non cambi durante la prova. Una modifica successiva rende l'evidenza da aggiornare. Conservare `passed`, `failed`, `not run` e il collegamento fra requisito, comando ed artefatto. Un giudizio LLM può integrare la revisione, ma non sostituisce il risultato delle prove.

4. **Controllare i passaggi di fase con lo stesso codice in CI.** Verificare chiarimenti irrisolti, requisiti senza task, evidenze mancanti o obsolete e percorsi di documentazione inesistenti. Distinguere un vincolo verificabile da una scelta di prodotto che richiede giudizio. Conservare le eccezioni della costituzione. Evitare ulteriori approvazioni per operazioni locali già autorizzate.

5. **Definire il comportamento dopo un fallimento.** Restituire step fallito, log, categoria del problema e comando utile. Per un errore deterministico richiedere una nuova ipotesi prima del tentativo successivo. Per un guasto ambientale transitorio consentire retry limitati. Quando si raggiunge il limite registrare prove e ostacolo, poi chiedere l'informazione o la decisione effettivamente necessaria. Le soglie vanno calibrate sui fallimenti del progetto; le fonti non stabiliscono un numero universale.

6. **Valutare prima di estendere il processo.** Usare la task bank già presente con baseline, ambiente, modello e limiti stabili. Ripetere task IPC, core e GUI in worktree disposable. Misurare completamento e regressioni prima di tempo, token e interventi umani. Confrontare una modifica del processo alla volta e ispezionare gli esiti e le tracce. Rimuovere regole che non migliorano i risultati o che i nuovi strumenti rendono superflue.

Il primo intervento dovrebbe chiarire indici, ownership e stato delle spec. Il comando di ingresso viene dopo, con una spec dedicata che riusi setup, spec-kit e verifica. La ricerca non giustifica introdurre un nuovo orchestratore né aumentare automaticamente il numero delle skill.
