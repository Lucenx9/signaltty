# Repository adatto agli agenti: evidenze e priorità per signaltty

Ricerca del 3 ottobre 2026. Questa nota integra la valutazione discussa in conversazione con fonti primarie. Le proposte sono da confrontare con il checkout al momento dell'implementazione: sono presenti modifiche in corso e questa ricerca non esegue test né certifica lo stato del prodotto.

## Le fonti e i loro limiti

### GitHub: preparare l'ambiente prima del lavoro

La documentazione ufficiale prevede un workflow `.github/workflows/copilot-setup-steps.yml` con un unico job `copilot-setup-steps`. Installa strumenti e dipendenze prima che Copilot inizi a lavorare; il file deve essere nel branch predefinito. È un meccanismo specifico di Copilot, ma suggerisce una scelta trasferibile: il setup del progetto deve essere eseguibile e verificabile, senza ricostruirlo a ogni sessione. [GitHub, Configure the development environment](https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/customize-cloud-agent/customize-the-agent-environment).

Per signaltty proporrei un setup comune richiamabile da diversi ambienti, con un controllo separato di Rust, librerie GTK/VTE, D-Bus e strumenti QA. Non è necessario adottare Copilot per ottenere questo risultato.

### ETH Zurich e LogicStar: misurare l'effetto delle istruzioni

La versione 3 di *Evaluating AGENTS.md*, aggiornata il 29 settembre 2026, non trova miglioramenti generali significativi nella risoluzione dei task con i file di contesto, mentre riporta un aumento medio dei costi superiore al 20%. Gli autori raccomandano istruzioni necessarie e non duplicate dal README. L'esperimento riguarda task Python, non la qualità complessiva di un prodotto Rust/GTK: non dimostra che `AGENTS.md` sia inutile in signaltty. [Gloaguen et al., versione 3](https://arxiv.org/html/2602.11988v3).

La conseguenza proposta è valutare le istruzioni su task reali del progetto, conservando convenzioni non ovvie e comandi affidabili. Il numero di skill installate non misura la qualità ottenuta.

### OpenAI: rendere osservabili applicazione e vincoli

Il resoconto dell'11 febbraio 2026 descrive istanze dell'applicazione per worktree, log e metriche accessibili agli agenti, documentazione indicizzata e controlli automatici dei confini architetturali. È un'esperienza interna, non un confronto sperimentale che garantisca gli stessi risultati altrove. [OpenAI, Harness engineering](https://openai.com/index/harness-engineering/).

In signaltty questo orienta verso scenari QA isolati e controlli delle dipendenze fra crate. Per esempio, un controllo con `cargo metadata` potrebbe verificare i confini dichiarati per core, proto e GUI. Socket, directory di stato e identità dell'applicazione devono consentire prove concorrenti indipendenti.

### Anthropic: valutare con criteri espliciti

Il resoconto del 24 marzo 2026 distingue implementazione e valutazione. Riporta problemi nell'autovalutazione e propone criteri di design espliciti, verifiche funzionali e passaggi di consegne strutturati. Un valutatore separato resta fallibile e richiede calibrazione. Sono esperimenti su applicazioni web: l'efficacia non è automaticamente trasferibile a GTK. [Anthropic, Harness design for long-running application development](https://www.anthropic.com/engineering/harness-design-long-running-apps).

Per signaltty proporrei criteri osservabili per gerarchia visiva, leggibilità, focus e completamento delle azioni. Screenshot e giudizi dell'agente devono accompagnare prove di comportamento, con revisione umana per le decisioni di prodotto.

### GNOME: qualità nativa e accessibilità

Le HIG richiedono nomi accessibili descrittivi e suggeriscono prove con contrasto elevato, testo ingrandito, navigazione da tastiera, screen reader e tastiera a schermo. Sono criteri direttamente pertinenti a GTK. [GNOME HIG, Accessibility](https://developer.gnome.org/hig/guidelines/accessibility.html).

La matrice QA di signaltty dovrebbe distinguere le prove automatizzate da quelle manuali, registrando quali scenari sono stati effettivamente eseguiti.

## Priorità proposte

Queste priorità sono una sintesi progettuale per signaltty, non risultati dimostrati dalle fonti:

1. Rendere setup e verifica eseguibili da un checkout pulito. Usare gli stessi comandi nell'ambiente dell'agente e in CI.
2. Rendere ripetibili i flussi GUI principali e conservare log, screenshot e risultati dei controlli. Un'immagine corretta non prova che il flusso funzioni.
3. Controllare automaticamente i vincoli architetturali già dichiarati e mantenere una sola fonte per ogni regola.
4. Ridurre gli obblighi di lettura generici. Caricare le istruzioni pertinenti al task e verificare che i percorsi funzionino nei diversi harness.
5. Valutare il processo su 5–10 task reali con ambiente e criteri stabili, ripetendo le prove. Misurare completamento, interventi umani, difetti, tempo di setup e instabilità dei test prima di aggiungere altre regole.

I punti di partenza locali sono l'[indice della documentazione](../README.md), la [direzione del prodotto](../14-product-direction.md) e il [routing delle skill](../15-agent-skills.md). Quest'ultimo prescrive già numerose skill per fase: la proposta è verificarne l'effetto, senza presumere che aumentarne il numero migliori il prodotto.
