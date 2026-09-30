# SPDX-License-Identifier: MPL-2.0
# examples/codehub/texts.py -- every text of the page (read by build.py)
# name -> text; expanded into `static NAME: [u8; N] = "..."` + const NAME_N
T = [
 ("T_BRAND", "CodeHub"),
 ("T_LINK1", "Erkunden"),
 ("T_LINK2", "Hilfe"),
 ("T_LOGIN", "Anmelden"),
 ("T_PILL", "Testbetrieb · Made in Tirol"),
 ("T_H1", "Code."),
 ("T_H2", "Made in Austria."),
 ("T_SUB", "CodeHub ist die Code-Plattform aus Österreich: Repositories, Pull Requests, CI und Pakete – offen, souverän und nach EU-Recht betrieben."),
 ("T_BTN1", "Anmelden  →"),
 ("T_BTN2", "Projekte entdecken"),
 ("T_TICK", "✓"),
 ("T_C0", "DSGVO"),
 ("T_C1", "Kein US CLOUD Act"),
 ("T_C2", "100 % Open Source"),
 ("T_C3", "Support auf Deutsch"),
 ("T_C4", "ID Austria (bald)"),
 ("T_TTITLE", "~/mein-projekt — zsh"),
 ("T_PROMPT", "$"),
 ("T_L0", "git remote add origin ssh://git@codehub.at/team/app.git"),
 ("T_L1", "git push -u origin main"),
 ("T_L2", "Objekte werden gezählt: 1.337, fertig."),
 ("T_L3", "Komprimiere Objekte: 100% (842/842), fertig."),
 ("T_L4", "To codehub.at:team/app.git"),
 ("T_L5", " * [new branch]      main -> main"),
 ("T_L6", "✓ Dein Code liegt jetzt bei CodeHub"),
 ("T_S0V", "Tirol"),
 ("T_S0C", "Betreiber · Server in Tirol (geplant)"),
 ("T_S1V", "0"),
 ("T_S1C", "US-Tracker & Werbung"),
 ("T_S2V", "1 Klick"),
 ("T_S2C", "Import von GitHub/GitLab"),
 ("T_S3V", "GPLv3+"),
 ("T_S3C", "Forgejo-Basis, offen & frei"),
 ("THEME_TXT", open(os.path.join(H, 'codehub.theme')).read()),
 ("SVG_LOGO", open(os.path.join(H, 'logo.svg')).read().strip()),
 ("SVG_HILLS", open(os.path.join(H, 'hills.svg')).read().strip()),
 ("SVG_LOGIN", "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='#F4F5F7' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><path d='M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4'/><polyline points='10 17 15 12 10 7'/><line x1='15' y1='12' x2='3' y2='12'/></svg>"),
]
def esc(s):
    return s.replace('\\','\\\\').replace('"','\\"').replace('\n','\\n')
def statics():
  out=[]
  for n,t in T:
    b=len(t.encode())
    out.append(f'static {n}: [u8; {b}] = "{esc(t)}"')
    out.append(f'const {n}_N: usize = {b}')
  return "\n".join(out)
