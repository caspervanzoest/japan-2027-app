# japan-2027-app

Persoonlijke reisapp voor een reis door Japan in 2027. Een PWA die je op het beginscherm van een iPhone zet, plus een klein hulpprogramma voor de server.

Deze repo bevat alleen code. De reisdata staat in een aparte privérepo. De app laadt die pas als je een eigen token invult.

- `docs/`: de PWA, gepubliceerd via GitHub Pages. Voor nu is het een testpagina die nagaat of alle verbindingen werken: installeren en offline, de reisdata op GitHub, de AI-assistent en pushmeldingen.
- `agent/`: `japan-agent`, een Rust-programma dat op de server draait.
  - `sync` houdt de reisdata gelijk met GitHub.
  - `pair` maakt een eenmalige koppelcode voor de app.
  - `push` stuurt een pushmelding.
