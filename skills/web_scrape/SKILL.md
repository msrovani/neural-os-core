---
name: web_scrape
description: Busca, extrai e resume conteudo de paginas web para o usuario
required_tokens: [1]
requires_network: true
---

# Web Scrape Skill

Quando o usuario pedir para verificar, buscar, olhar, ou trazer informacoes de um site (ex: "veja o que tem de novo no site G1", "me mostra a cotacao do dolar", "busca tal noticia"), siga este workflow.

## Workflow

### 1. Identificar o Alvo

Extraia o nome do site ou URL da pergunta. Se ambiguo, pergunte. Resolva atalhos conhecidos:

| Atalho | URL |
|--------|-----|
| G1 | http://g1.globo.com |
| UOL | http://www.uol.com.br |
| Wikipedia / wiki | http://pt.wikipedia.org |
| GitHub | http://github.com |
| Globo | http://www.globo.com |
| CNN | http://www.cnn.com |
| BBC | http://www.bbc.com |
| Stack Overflow / SO / stack | http://stackoverflow.com |
| Reddit | http://www.reddit.com |

Se o usuario der URL direta (com ou sem `http://`), normalize e use. Se houver duvida entre multiplas interpretacoes, liste opcoes.

### 2. Fetch com Controle

Use o comando `/scrape <url>` (ou `/web`, `/site`). Parametros implicitos:
- `timeout`: 30s por pagina
- `user-agent`: "NeuralOS-Hermes/1.0"
- `max_paginas`: 1 (para crawl profundo, o usuario precisa pedir explicitamente)

### 3. Extracao Fit Markdown

Transforme o HTML em Markdown limpo:

1. **Titulo**: `# {title}` do `<title>` ou `<h1>`
2. **Metadata**: data, autor, url em blockquote
3. **Conteudo principal**: identifique `<article>`, `<main>`, ou a maior secao de texto — ignore menus, sidebar, footer, ads
4. **Headings**: `<h1-h6>` → `#` a `######`
5. **Paragrafos**: um por linha
6. **Links**: mantenha como `[texto](url)` apenas se relevantes ao conteudo
7. **Listas**: `<ul>` → `- item`, `<ol>` → `1. item`
8. **Tabelas**: formato pipe `| col1 | col2 |`
9. **Codigo**: `<code>` → `` `inline` ``, `<pre>` → ``` bloco ```
10. **Citacoes**: `<blockquote>` → `> citacao`
11. **Imagens**: `![alt](src)` apenas se relevantes

### 4. Remover Propaganda

Remova antes da filtragem geral:
- Marcadores: `[PUBLICIDADE]`, `[PUBLI]`, `[ANUNCIO]`, `[PATROCINIO]`, `[AD]`
- Secoes com classe/ID: `ad`, `ads`, `advert`, `banner`, `sponsor`, `propaganda`, `patrocinio`, `publi`, `comercial`
- Intersticiais: "continua apos publicidade", "continue lendo apos o anuncio"
- Links pagos: "saiba mais", "aproveite", "oferta", "compre agora", "clicou e viu", "ligue ja"
- Iframes de ad, "veja tambem" com links comerciais

### 5. Filtragem por Relevancia

- Remova linhas com <30 caracteres (provavel navegacao), exceto headings e list items
- Remova duplicatas consecutivas (menus repetidos)
- Se >4000 caracteres, priorize os primeiros 2000 do conteudo principal + headings
- Se o usuario pediu algo especifico, preserve apenas secoes com palavras-chave relevantes

### 6. Extracao Estrutural (JSON opcional)

Se o usuario pedir dados estruturados (cotacao, clima, tabela), formate como JSON:

```json
{
  "title": "Titulo da Pagina",
  "url": "http://...",
  "extracted_at": "2026-07-24 18:30",
  "source": "g1.globo.com",
  "content_markdown": "# Titulo\n\nConteudo...",
  "topics": ["topico1", "topico2"],
  "structured_data": {
    "cotacao": 5.42,
    "variacao": "+0.32%"
  },
  "links": [
    {"text": "Noticia 1", "url": "http://..."}
  ],
  "metadata": {
    "author": "Nome",
    "published": "2026-07-24",
    "word_count": 1240
  }
}
```

### 7. Sumarizar

Produza um resumo de 3-7 linhas:
- O que e a pagina (contexto)
- Pontos principais organizados por topico
- Dados numericos/destaques em **negrito**
- Se noticia: o que, onde, quando, quem
- Se cotacao: valor + variacao + tendencia
- Se lista/tabela: bullet points
- Se pergunta especifica: filtre para responder apenas aquilo

### 8. Output

Apresente como `#[Hermes] Resumo do {site}:`, tom natural, fonte e data. Ofereça acoes:
- "Quer que eu leia alguma noticia especifica?"
- "Quer que eu busque mais detalhes sobre algum topico?"
- "Quer que eu formate como JSON?"
- "Quer que eu verifique outros sites tambem?"

### 9. Crawling Recursivo (so se solicitado)

Quando usuario pedir "explora", "crawleia", "mais paginas":
- Siga links internos (mesmo dominio) ate profundidade 2
- Limite: max 5 paginas por request
- Delay 500ms entre requests (polite crawling)
- Evite duplicatas
- Retorne sumario multi-pagina com titulo de cada + trecho relevante

## Tratamento de Erros

- **404 / timeout / DNS**: mensagem descritiva ("pagina nao encontrada", "site nao respondeu", "site nao encontrado no DNS")
- **Conteudo nao HTML**: avise que nao e uma pagina web (PDF, imagem, binario)
- **Pagina muito grande** (>100KB): avise e extraia apenas os primeiros 2000 caracteres uteis
- **Site com JS pesado**: "O site X depende de JavaScript para renderizar. Nao consigo extrair o conteudo completo."
- **Site bloqueou**: "O site X rejeitou a requisicao. Pode ser bloqueio por user-agent ou Cloudflare."
- **Rede indisponivel**: "Rede nao disponivel. Verifique se o sistema esta conectado (e1000 com `--nic-promisc1 allow-all` no VBox)"

## Recorrencia e Aprendizado Adaptativo

- **2ª vez no mesmo site**: compare com a visita anterior, destaque o que mudou
- **3ª vez (padrao)**: sugira criar um cron ou App para monitoramento automatico
- **Mudanca de layout**: se a extracao ficar diferente, avise "O layout do {site} mudou — ajustando extracao..."
- **Gatilho proativo**: se o usuario sempre pede o mesmo site no mesmo horario, ofereca automatizar
- **Multiplas fontes**: se visita 3+ sites de noticia, ofereca resumo agregado

## Exemplos

Input: "veja o que tem de novo no site G1"
Output:
  "# Resumo do G1
   > Fonte: g1.globo.com | Extraido em 2026-07-24 18:30

   ## Principais noticias de hoje
   - **Governo anuncia novo pacote economico** — medidas fiscais
   - **Tecnologia**: Startup brasileira levanta R$ 200mi
   - **Esportes**: Selecao se prepara para amistoso

   _Quer que eu abra alguma noticia especifica?_"

Input: "cotacao dolar hoje"
Output:
  "# Cotacao do Dolar
   > Fonte: Google Finance | 2026-07-24

   Dolar Comercial: **R$ 5,42** (+0,32%)
   Dolar Turismo: **R$ 5,58** (+0,28%)

   _Quer que eu formate como JSON?_"

Input: "explora o site da CNN sobre tecnologia"
Output:
  "Explorando cnn.com em profundidade...

   ## CNN — Secao Tech (3 paginas)

   1. **Apple lanca MacBook Pro M4** — 30% mais rapido
   2. **Google atualiza Gemini** — IA com contexto
   3. **Tesla recall de 50 mil veiculos** — correcao de software

   Crawl: 3/5 paginas | 1.2s | 1 dominio"

Input: "me mostra o site tal"
Output:
  "Qual site exato voce quer ver?
   - `http://site-tal.com` (site exemplo)
   - `http://site-tal.org` (organizacao)
   - `http://site-tal.tech` (tecnologia)
   Ou me passe a URL completa."

## Regras de Seguranca

- Nao siga links para dominios externos sem confirmacao do usuario
- Nao faca crawl agressivo: max 5 paginas, delay de 500ms, respeite `robots.txt` quando disponivel
- Nao extraia conteudo de paginas de login, pagamento, ou areas restritas
- Nao armazene cache de paginas com dados pessoais
- Se o usuario pedir "todos os links da pagina", avise que isso pode incluir links de propaganda e navegacao
