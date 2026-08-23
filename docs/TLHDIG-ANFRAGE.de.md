# Anfrage an die TLHdig-Redaktion

Versandfertig. Der technische Befund darunter ist vollständig gemessen und in
`docs/FONTS.md` dokumentiert. Wird die Frage beantwortet, gehört die Antwort
dorthin.

Die Empfängeradresse stammt von der TLHdig-Seite des Portals, wo sie als
*Contact* ausgewiesen ist; die Adresse in Kopie ist das Impressum des
Hethitologie-Portals Mainz. Beide über `hethport.net` erreichbar — der Host
`hethport.uni-wuerzburg.de` verweigert den TLS-Handshake.

---

**An:** tlhdig@uni-wuerzburg.de
**Kopie:** gerfrid.mueller@adwmainz.de
**Betreff:** Fünf Codepunkte im Private-Use-Bereich von TLHdig Beta 0.3 ohne
bekannte Schriftart

Sehr geehrte Damen und Herren,

ich arbeite an *Aruna*, einer quelloffenen Software (MIT-Lizenz,
https://github.com/sergeyssimonov-max/Aruna), die die Transliterationen aus
TLHdig in eigenständige HTML-Dokumente und künftig in PDF überführt. Grundlage
ist TLHdig Beta 0.3 (Zenodo-Datensatz 20328284, Archiv-MD5
`f9acbc8db3111cc7dd88d82f7819a912`). Dabei ist mir ein Punkt aufgefallen, bei
dem ich ohne Ihre Auskunft nicht weiterkomme.

## Der Befund

Um sicherzustellen, dass sich jedes Zeichen des Korpus zuverlässig darstellen
lässt, habe ich sämtliche Codepunkte des Archivs erhoben und gegen die
`cmap`-Tabellen aller auf dem Rechner installierten Schriftarten geprüft — 648
verschiedene Codepunkte gegen 366 Schriftdateien.

Bis auf sechs Codepunkte ist alles abgedeckt. Fünf davon liegen im
Private-Use-Bereich der Ebene 16 und stehen in den `cu`-Attributen, also
innerhalb der zeichenweisen Keilschriftwiedergabe, zwischen gewöhnlichen
Zeichen des Blocks `U+12xxx`. Sie sind demnach Keilschriftzeichen, für die
Unicode keine Kodierung vorsieht.

| Codepunkt | Vorkommen | Zeilen | Dateien |
|---|---|---|---|
| `U+100009` | 2 715 | 2 379 | 976 |
| `U+100003` | 13 | 13 | 9 |
| `U+100006` | 3 | 3 | 3 |
| `U+100001` | 1 | 1 | 1 |
| `U+100005` | 1 | 1 | 1 |

Belegstellen, jeweils mit dem Inhalt des `cu`-Attributs:

- `U+100009` — CTH 572, KBo 58.64, Rs. IV 2′
  `𒌝𒈠𒋗𒉡𒈠𒐈𒋫𒁄􀀉𒌗𒉿𒋼𒀀𒊭𒀭`
- `U+100003` — CTH 777, KBo 62.22, 10′
  `𒄩𒇻𒌋𒊭􀀃𒈨𒇻𒌋𒊭`
- `U+100006` — CTH 628, DAAM 1.17, Vs. 2
  `𒃻𒄿𒁺𒊑𒂊𒌍𒂠𒁕𒁇𒊭𒀀𒋾𒁹𒁹𒃻􀀆𒂠𒁕`
- `U+100001` — CTH 470, KBo 25.102, Vs.? II 5′
  `▒𒄑􀀁𒀭𒁕▒`
- `U+100005` — CTH 389, KBo 52.18, 7′
  `▒▒▒𒉡𒉿𒄠𒈨𒂖𒍜􀀅𒊭𒊭▒▒`

## Was ich bereits geprüft habe

Keine der mir zugänglichen Schriftarten stellt diese fünf Zeichen dar:

- **Ullikummi A, B und C** in der Fassung, die derzeit über das
  Hethitologie-Portal ausgeliefert wird (Version 1.003 bzw. 1.002, SHA-256 des
  Pakets `28f8bb7ebc572009760066373edbf730c5bbcc2e974ec85109a6a44e5a2e55c7`).
  UllikummiA belegt im Private-Use-Bereich der Ebene 16 genau drei Positionen:
  `U+100000`, `U+100007` und `U+10000A`.
- **Semiramis Unicode 3**
- sämtliche 366 mit macOS 13 ausgelieferten Schriftarten, darunter
  Noto Sans Cuneiform

Auch die dem Schriftpaket beiliegende Zeichenliste (`HittiteSignList.pdf` aus
`SignLists.zip`) führt genau dieselben drei Private-Use-Positionen auf —
`U+100000`, `U+100007`, `U+10000A` — und zwar in der Tabelle unmittelbar neben
den regulären `U+12xxx`-Zeichen. Von diesen dreien verwendet das Korpus nur
`U+100000`; die fünf oben genannten kommen dort nicht vor.

Die Belegung des Private-Use-Bereichs in TLHdig geht damit über das hinaus, was
im Umfeld der Schriftarten veröffentlicht ist. Auf welche Weise ich das noch
selbst hätte klären können, ist mir nicht ersichtlich.

## Meine Frage

1. Um welche Keilschriftzeichen handelt es sich bei `U+100001`, `U+100003`,
   `U+100005`, `U+100006` und `U+100009`?
2. Gibt es eine Schriftart, die diese Positionen darstellt — etwa eine neuere
   oder projektinterne Fassung von Ullikummi —, und wäre sie zugänglich?
3. Existiert eine Dokumentation der TLHdig-eigenen Private-Use-Belegung, die
   ich übersehen habe?

## Warum ich nicht selbst eine Lösung wähle

Naheliegend wäre, ein ähnlich aussehendes Zeichen einzusetzen oder die
betreffenden Stellen zu übergehen. Beides habe ich bewusst unterlassen. Der
XML-Quelltext bleibt in meiner Verarbeitung unverändert, und ein falsch
dargestelltes Zeichen wäre gravierender als ein fehlendes: Das fehlende ist als
Lücke erkennbar, das falsche wird gelesen.

Gegenwärtig erscheinen die fünf Zeichen unter macOS als Platzhalterkästchen der
Systemschrift `LastResort`. In einem PDF wären sie je nach Renderer entweder
ein solches Kästchen oder gar nichts — in 2 379 Zeilen.

## Ergänzende Angaben

Eine vollständige Liste aller Fundstellen mit Textzeugen, Zeilenangaben und dem
jeweiligen Kontext kann ich Ihnen gern zusenden; ebenso die Erhebung sämtlicher
648 Codepunkte des Korpus mit Häufigkeiten. Das Prüfprogramm ist Teil des oben
genannten Repositoriums und damit für Sie einsehbar und nachvollziehbar; die
hier genannten Zahlen lassen sich mit einem einzigen Befehl reproduzieren.

Für eine Rückmeldung wäre ich Ihnen sehr verbunden.

Mit freundlichen Grüßen

Sergey Simonov
sergey.s.simonov@gmail.com
https://github.com/sergeyssimonov-max/Aruna
