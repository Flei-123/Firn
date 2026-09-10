#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/english/check_comments2.py -- THE SECOND NET UNDER THE COMMENTS.

check_comments.py decides by GERMAN FUNCTION WORDS (der, die, und, nicht...).
That is the right net for prose, and it deliberately holds no word that also
exists in English. Two kinds of German line slip through it:

  * ALL-CAPS HEADINGS -- "DIE GEMEINSAME REGEL", "DIE GRIFFRILLEN". They carry
    a function word, but the line is a heading, and after the prose around it
    is translated the heading is the only German left.
  * ONE-WORD LINES -- "Senkrechte Streifen.", "Waagerechte Streifen." No
    function word at all, so the first net cannot see them.

This net decides by CONTENT WORDS instead, against tools/english/morphemes.tsv
-- the same table check_names.py uses for path names. A word counts as German
when it stands in that table, is not a whole English word that happens to
contain a German morpheme (EN_OK), and is not inside `back ticks` or a path.

  python3 tools/english/check_comments2.py [file...]      default: lib/fui/*.fi

Both nets have to report zero. Neither is enough on its own.
"""
import re,sys,glob,os
mor=set()
for z in open('tools/english/morphemes.tsv',encoding='utf-8'):
    if z.strip() and not z.startswith('#'):
        p=z.rstrip('\n').split('\t')
        a=p[0].strip().lower(); b=(p[1].strip().lower() if len(p)>1 else '')
        if a and b and a!=b.replace('_',''): mor.add(a)
# English words that literally contain a german morpheme -> ignore as whole words
EN_OK={'lies','lie','lied','dies','grab','grabs','grabbed',
 'absolute','relative','negative','signature','aggregate','profile','surrogate',
 'parameter','alternative','imperative','declarative','iterative','mark','marker','marks',
 'rand','random','band','hand','land','stand','brand','grand','strand','send','end','ends',
 'bind','find','kind','mind','wind','round','sound','found','ground','bound','pound','and',
 'best','rest','test','list','fist','last','fast','cast','past','vast','most','post','host',
 'art','part','start','cart','chart','smart','apart','was','war','warn','warning','wart',
 'ist','list','exist','fill','will','still','all','ball','call','fall','hall','small','tall',
 'wall','well','tell','sell','cell','bell','fell','hell','shell','spell','smell','swell',
 'in','on','an','man','can','ran','tan','van','pan','plan','span','scan','than','then',
 'is','it','at','as','so','no','of','or','to','do','go','be','by','we','he','me','my',
 'the','for','not','but','out','our','one','two','six','ten','new','old','own','use','way',
 'who','why','how','now','see','set','get','let','put','run','sum','top','bit','bug','bus',
 'dot','cut','fit','gap','hit','job','key','lot','map','max','min','mix','net','odd','off',
 'per','pin','ram','raw','red','row','tab','tag','tip','ton','via','win','yes','zip','arm',
 'bar','box','buf','cap','col','cpu','day','dir','doc','err','fix','gcc','hex','idx','img',
 'int','len','lib','log','mem','mod','msg','num','obj','opt','pos','ptr','ref','reg','ret',
 'rgb','seq','src','std','str','sub','tmp','val','var','vec','win','xml','zoom','data',
 'name','names','value','values','line','lines','file','files','number','numbers','word',
 'words','text','texts','size','sizes','time','times','type','types','code','codes',
 'point','points','area','areas','list','lists','case','cases','item','items','base',
 'basis','element','elements','listen','listener','lang','side','sides','half','halves',
 'note','notes','order','orders','field','fields','level','levels','state','states',
 'stage','stages','step','steps','rule','rules','role','roles','mode','modes','link',
 'links','load','loads','path','paths','port','ports','rate','rates','root','roots',
 'sign','signs','site','sites','slot','slots','span','spans','tree','trees','unit',
 'units','user','users','view','views','wave','waves','zone','zones','pixel','pixels',
 'font','fonts','grid','grids','icon','icons','menu','menus','page','pages','pane',
 'panes','tile','tiles','node','nodes','edge','edges','face','faces','form','forms',
 'game','games','goal','goals','hint','hints','host','hosts','idea','ideas','kernel',
 'label','labels','layer','layers','match','model','models','month','months','mouse',
 'movie','music','north','offer','onset','other','panel','panels','paper','phase',
 'photo','piece','pixel','place','plane','plant','plate','press','price','prime','print',
 'prior','probe','proof','quota','radio','range','ratio','reach','ready','realm','refer',
 'relay','reset','right','ring','rings','rough','round','route','scale','scene','scope',
 'score','screen','sense','serve','shape','share','sheet','shell','shift','shore','short',
 'sight','since','slant','slide','slope','solid','sound','south','space','spare','speed',
 'spend','split','stack','staff','stage','stamp','stand','start','state','steam','steel',
 'stern','stick','still','stock','stone','store','storm','story','strip','study','style',
 'sugar','suite','super','sweet','table','taken','taste','teach','texts','thank','theme',
 'there','these','thick','thing','think','third','those','three','throw','tight','timer',
 'title','today','token','topic','total','touch','tough','tower','trace','track','trade',
 'trail','train','treat','trend','trial','tribe','trick','trust','truth','twice','under',
 'union','unite','until','upper','urban','usage','usual','valid','value','video','virus',
 'visit','vital','voice','waste','watch','water','weird','wheel','where','which','while',
 'white','whole','whose','width','world','worry','worse','worth','would','wound','write',
 'wrong','yield','young','yours','draw','drawn','drop','dual','dump','each','east','easy',
 'edit','else','even','ever','exit','face','fact','fail','fair','fall','fast','fear',
 'feed','feel','feet','fell','felt','file','fill','film','find','fine','fire','firm',
 'fish','five','flat','flow','fold','folk','food','foot','ford','fork','form','fort',
 'four','free','from','fuel','full','fund','gain','game','gate','gave','gear','gene',
 'gift','girl','give','glad','goal','goes','gold','golf','gone','good','gray','grew',
 'grey','grid','grow','gulf','hair','half','hall','halt','hand','hang','hard','harm',
 'hate','have','head','hear','heat','held','hell','help','here','hero','hide','high',
 'hill','hint','hire','hold','hole','holy','home','hope','horn','host','hour','huge',
 'hung','hunt','hurt','idea','inch','into','iron','item','join','joke','jump','just',
 'keep','kept','kick','kill','kind','king','knee','knew','know','lack','lady','laid',
 'lake','lamp','land','lane','last','late','lead','leaf','lean','leap','left','lend',
 'less','lift','like','limb','lime','line','link','lion','list','live','load','loan',
 'lock','loco','long','look','loop','lord','lose','loss','lost','loud','love','luck',
 'made','mail','main','make','male','mall','many','mark','mask','mass','mate','math',
 'meal','mean','meat','meet','melt','mere','mesh','mess','mild','mile','milk','mill',
 'mind','mine','miss','mode','mood','moon','more','most','move','much','must','myth',
 'nail','name','navy','near','neat','neck','need','news','next','nice','nine','node',
 'none','noon','norm','nose','note','noun','okay','once','only','onto','open','oral',
 'over','pace','pack','page','paid','pain','pair','pale','palm','park','part','pass',
 'past','path','peak','pick','pile','pink','pipe','plan','play','plot','plug','plus',
 'poem','poet','pole','poll','pond','pool','poor','pope','port','pose','post','pour',
 'pray','prep','prey','pull','pump','pure','push','quit','race','rack','rage','raid',
 'rail','rain','rank','rare','rate','read','real','rear','rely','rent','rest','rice',
 'rich','ride','ring','riot','rise','risk','road','rock','role','roll','roof','room',
 'root','rope','rose','rule','rush','safe','said','sail','sake','sale','salt','same',
 'sand','save','seal','seat','seed','seek','seem','seen','self','sell','send','sent',
 'ship','shoe','shop','shot','show','shut','sick','side','sign','silk','sing','sink',
 'site','size','skin','skip','slip','slow','snap','snow','soap','sock','soft','soil',
 'sold','sole','solo','some','song','soon','sort','soul','soup','spot','star','stay',
 'stem','step','stir','stop','such','suit','sure','swim','tail','take','tale','talk',
 'tall','tank','tape','task','team','tear','tell','tend','tent','term','test','text',
 'than','that','them','then','they','thin','this','thus','tide','tidy','tied','tile',
 'till','time','tiny','tire','told','toll','tone','tool','torn','tour','town','tram',
 'trap','tray','tree','trim','trip','true','tube','tune','turn','twin','type','ugly',
 'unit','upon','urge','used','user','vary','vast','verb','very','vice','view','vote',
 'wage','wait','wake','walk','wall','want','ward','warm','warn','wash','wave','ways',
 'weak','wear','week','well','went','were','west','what','when','whom','wide','wife',
 'wild','will','wind','wine','wing','wipe','wire','wise','wish','with','wood','wool',
 'word','wore','work','worn','wrap','yard','yeah','year','your','zero','zone','zoom'}
def parts(s):
    return re.findall(r"[A-Za-zÄÖÜäöüß]+", s)
def scan(paths):
    out=[]
    for f in paths:
        for i,line in enumerate(open(f,encoding='utf-8',errors='ignore'),1):
            s=line.rstrip('\n')
            if not s.strip().startswith('//'): continue
            probe=re.sub(r'`[^`]*`',' ',s)
            probe=re.sub(r'\b(?:lib|tools|bin|tests|docs|src)/[A-Za-z0-9_./-]+',' ',probe)
            # Absolute paths OUTSIDE this tree are foreign names. The button
            # template lives at /root/jarvis/downloads/knopf-vorlage/ and is
            # not ours to rename -- the mechanical rename turned "knopf" into
            # "button" inside that path once and broke the reference.
            probe=re.sub(r'/root/[A-Za-z0-9_./-]+',' ',probe)
            bad=[]
            for w in parts(probe):
                lw=w.lower()
                if lw in EN_OK or len(lw)<3: continue
                if 'ä' in lw or 'ö' in lw or 'ü' in lw or 'ß' in lw: bad.append(w); continue
                if lw in mor: bad.append(w)
            if bad: out.append((f,i,bad,s.strip()))
    return out
if __name__=='__main__':
    paths=sys.argv[1:] or sorted(glob.glob('lib/fui/*.fi'))
    r=scan(paths)
    print("SUSPECT GERMAN COMMENT LINES (morpheme net): %d"%len(r))
    cur=None
    for f,i,bad,s in r:
        if f!=cur: print("\n--- %s ---"%f); cur=f
        print("%5d %-28s %s"%(i,','.join(sorted(set(bad))[:4]),s[:95]))
