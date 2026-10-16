# japan-2027-app

Persoonlijke reisapp voor een reis door Japan in 2027. Een PWA die je op het beginscherm van een iPhone zet, plus een klein hulpprogramma voor de server.

Deze repo bevat alleen code. De reisdata staat in een aparte privérepo. De app laadt die pas als je een eigen token invult. Beheer en tokens staan beschreven in de privérepo.

## Onderdelen

- `docs/`: de PWA, gepubliceerd via GitHub Pages (branch `main`, map `/docs`). Voor nu is het een kale chat met de AI-assistent. Onder het tandwiel staan de koppeling met T3, het GitHub-token en pushmeldingen.
- `agent/`: `japan-agent`, een Rust-programma dat op de server draait.

## Hoe de PWA werkt

**Reisdata.** De app gebruikt de contents-API van GitHub, met een fine-grained token dat alleen bij de privérepo kan. Hij leest met `cache: 'no-store'`, want GitHub laat de browser antwoorden 60 seconden bewaren en een verouderde `sha` geeft bij het schrijven een 409. Bij een 409 haalt de app de nieuwe `sha` op en probeert het opnieuw.

**Assistent.** De app praat met een eigen T3 Code-server (T3 0.1.0, orchestratieprotocol 2), bereikbaar via T3 Connect. Het is de interne API van T3, die per versie kan veranderen. Daarom staat de T3-versie op de server vast.

- Koppelen: de gebruiker plakt `adres#code` of `adres#token`.
  - Een `code` is een eenmalige pairingcode. Die wisselt de app in via `POST /oauth/token` (token-exchange, `subject_token_type=urn:t3:params:oauth:token-type:environment-bootstrap`, `scope=orchestration:read orchestration:operate`) voor een bearer-token van 30 dagen.
  - Een langer `token` controleert de app met `GET /api/auth/session` en bewaart hij zoals het is.
- Een bericht sturen: `POST /api/auth/websocket-ticket`, dan een WebSocket naar `/ws?orchestrationProtocol=2&wsTicket=…`. Commando's gaan als Effect RPC (`{_tag:"Request", id, tag:"orchestration.dispatchCommand", payload, headers:[]}`), eerst `thread.create`, dan `message.dispatch`. Op `Ping` antwoordt de app met `Pong`. Een HTTP-route voor commando's bestaat in protocol 2 niet meer.
- De app leest het gesprek met `GET /api/orchestration/threads/:id` en de header `x-t3-orchestration-protocol: 2`. De berichten staan in `projection.messages`, met `role` `user` of `assistant`. Na het versturen vraagt de app dit elke 1,5 seconde op. Dat stopt zodra de run voor je eigen bericht (`userMessageId`) in `projection.runs` staat en klaar is. Zo verschijnt het antwoord terwijl het binnenkomt. Komt de app terug uit de achtergrond, dan haalt hij het gesprek opnieuw op.
- De app gebruikt één thread tot je op "Nieuw gesprek" drukt. Het laatst bekende gesprek staat ook in `localStorage`, zodat je het offline kunt teruglezen.
- CORS staat op de T3-server open voor elke origin.

**Offline.** De service worker bewaart de app-schil (stale-while-revalidate) en laat API-verzoeken met rust. GitHub Pages stuurt `max-age=600`. Daarom vraagt de service worker altijd met `cache: 'no-cache'` en installeert hij met `cache: 'reload'`, anders kan een update tot 10 minuten en een paar keer openen uitblijven. Wordt een nieuwe versie actief, dan herlaadt de pagina zich één keer.

**Push.** Web Push met VAPID. De app schrijft zijn inschrijving naar de privérepo (`app/push-subscription.json`). Op iOS kan dat alleen vanuit de app op het beginscherm.

Tokens staan in `localStorage`. Die opslag is gedeeld met alle Pages-sites onder hetzelfde `*.github.io`-adres.

## japan-agent

```bash
cd agent && cargo install --path . --root ~/.cargo
```

| Opdracht | Doet |
|---|---|
| `japan-agent sync` | `git pull --rebase --autostash` in de reisdatarepo, daarna pushen als er lokale commits zijn. Alle git-acties lopen via `flock`, omdat gelijktijdige fetches anders "Cannot rebase onto multiple branches" geven. |
| `japan-agent pair` | Eenmalige pairingcode van 10 minuten, beperkt tot `orchestration:read/operate`. Drukt `adres#code` af. |
| `japan-agent push <titel> <tekst> [url]` | Pushmelding naar de opgeslagen inschrijving. Bij 404 of 410 is de inschrijving verlopen. |

Instellingen via omgevingsvariabelen:

- `JAPAN_DATA`: de reisdatarepo
- `JAPAN_CONFIG`: map met `vapid-private-pkcs8.pem` en `t3-host`
- `JAPAN_T3_HOME`: datamap van de T3-server
