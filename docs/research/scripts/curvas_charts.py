import numpy as np, matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
from datetime import date, timedelta
plt.rcParams.update({"font.family":"DejaVu Sans","font.size":11,"axes.spines.top":False,"axes.spines.right":False})

# ---------------- Gráfico 1: dados reais (somente pontos com fonte) ----------------
def yrs(launch, d): return (d - launch).days/365.25
D=date
nets = {
 # nome: (lançamento, [(data, usuários_milhões, tipo)], cor)
 "Facebook (fev/2004)": (D(2004,2,4), [(D(2004,12,1),1,"r"),(D(2008,8,26),100,"a"),(D(2012,10,4),1000,"m"),(D(2023,6,30),3030,"m")], "#1F3B8F"),
 "WhatsApp (2009)": (D(2009,5,1), [(D(2013,4,16),200,"m"),(D(2013,12,19),400,"m"),(D(2016,2,1),1000,"m"),(D(2020,2,12),2000,"m")], "#25D366"),
 "Instagram (out/2010)": (D(2010,10,6), [(D(2010,12,12),1,"r"),(D(2013,2,26),100,"m"),(D(2018,6,20),1000,"m"),(D(2022,10,26),2000,"m")], "#C13584"),
 "Telegram (ago/2013)": (D(2013,8,14), [(D(2015,5,1),62,"m"),(D(2016,2,23),100,"m"),(D(2018,3,22),200,"m"),(D(2020,4,24),400,"m"),(D(2021,1,12),500,"m"),(D(2022,6,19),700,"m"),(D(2024,3,15),900,"m"),(D(2024,7,22),950,"m"),(D(2025,3,19),1000,"m")], "#29B6F6"),
 "Discord (mai/2015)": (D(2015,5,13), [(D(2016,1,15),3,"r"),(D(2016,12,9),25,"r"),(D(2017,12,1),87,"r"),(D(2020,6,30),100,"m"),(D(2021,9,1),150,"m")], "#5865F2"),
 "Bluesky (beta fev/2023)": (D(2023,2,17), [(D(2024,9,4),8.8,"r"),(D(2024,9,6),9,"r"),(D(2024,9,16),10,"r"),(D(2024,11,19),20,"r"),(D(2025,1,29),30,"r"),(D(2025,10,15),40,"r")], "#FF7A00"),
}
fig, ax = plt.subplots(figsize=(13,7.5), dpi=150)
mk={"m":"o","r":"s","a":"D"}
for name,(l,pts,c) in nets.items():
    x=[yrs(l,d) for d,_,_ in pts]; y=[u*1e6 for _,u,_ in pts]
    ax.plot(x,y,"-",color=c,lw=2.2,alpha=.85,label=name)
    for (d,u,t),xi,yi in zip(pts,x,y): ax.plot(xi,yi,mk[t],color=c,ms=7,mec="white",mew=1)
ax.set_yscale("log"); ax.set_ylim(5e5,5e9); ax.set_xlim(0,20)
ax.set_xlabel("Anos desde o lançamento"); ax.set_ylabel("Usuários (escala log)")
ax.yaxis.set_major_formatter(matplotlib.ticker.FuncFormatter(lambda v,_: f"{v/1e9:g} bi" if v>=1e9 else f"{v/1e6:g} mi"))
ax.grid(True,which="major",alpha=.25)
ax.annotate("Bluesky: +3 mi em ~1 semana\n(bloqueio do X no Brasil, ago–set/2024;\n85% dos novos eram brasileiros)",xy=(yrs(D(2023,2,17),D(2024,9,6)),9e6),xytext=(2.3,1.2e6),fontsize=9,arrowprops=dict(arrowstyle="->",color="#FF7A00"),color="#0B3D91")
ax.annotate("Telegram: +25 mi em 72 h\n(nova política do WhatsApp, jan/2021)",xy=(yrs(D(2013,8,14),D(2021,1,12)),5e8),xytext=(9.3,1.3e8),fontsize=9,arrowprops=dict(arrowstyle="->",color="#229ED9"),color="#0B3D91")
ax.annotate("Facebook: só universidades\naté set/2006 (1 mi em dez/2004)",xy=(yrs(D(2004,2,4),D(2004,12,1)),1e6),xytext=(1.2,2.6e5*3),fontsize=9,arrowprops=dict(arrowstyle="->",color="#1F3B8F"),color="#0B3D91")
ax.annotate("Discord: nasceu em servidores\nde jogos (FFXIV, 2015)",xy=(yrs(D(2015,5,13),D(2016,1,15)),3e6),xytext=(3.2,1.4e7),fontsize=9,arrowprops=dict(arrowstyle="->",color="#5865F2"),color="#0B3D91")
from matplotlib.lines import Line2D
h,l=ax.get_legend_handles_labels()
h+= [Line2D([],[],marker="o",ls="",color="gray",label="MAU (ativos/mês)"),Line2D([],[],marker="s",ls="",color="gray",label="cadastrados/registrados"),Line2D([],[],marker="D",ls="",color="gray",label="ativos (definição não especificada)")]
ax.legend(handles=h,loc="lower right",fontsize=9.5,frameon=False,ncol=2)
ax.set_title("Curvas reais de crescimento: tempo desde o lançamento × usuários",fontsize=15,fontweight="bold",loc="left")
fig.text(0.01,0.01,"Somente pontos com fonte (ver docs/research/curvas-crescimento-redes.md). Linhas ligam marcos; não representam dados mensais. Métricas diferem entre redes (MAU × cadastrados).",fontsize=8.5,color="gray")
fig.tight_layout(rect=(0,0.03,1,1)); fig.savefig("docs/research/img/curvas-redes-reais.png"); plt.close(fig)

# ---------------- Gráfico 2: modelo ilustrativo ----------------
rng=np.random.default_rng(7)
start=D(2027,3,1); W=52*4  # 4 anos semanais
t=np.arange(W)/52.0  # anos
days=[start+timedelta(weeks=int(i)) for i in range(W)]
K=30e6; r=1.55; t0=3.0
base=K/(1+np.exp(-r*(t-t0)))
base=base-base[0]+40e3
y=base.copy()
ev=[]  # (semana, rótulo, deslocamento y do texto)
def bump(w0,amp,decay_w,keep,label=None,dy=1.0):
    # pico que decai, mantendo fração 'keep' como degrau permanente
    global y
    k=np.arange(W)-w0; m=k>=0
    y[m]+=amp*(keep+(1-keep)*np.exp(-k[m]/decay_w))
    if label: ev.append((w0,label,dy))
def wk(d): return int((d-start).days//7)
bump(0,450e3,3,0.25,"Lançamento: pico de curiosidade\n(imprensa + lista de espera)",1)
ev.append((7,"Vale pós-hype: só fica quem\ntem grupo/turma ativa (−55%)",1))
for yr in range(2027,2031):
    if yr>2027: bump(wk(D(yr,2,20)),0.04*base[wk(D(yr,2,20))]+150e3,6,0.7,"Volta às aulas (semestre 1)\n+ calouros" if yr==2028 else None)
    bump(wk(D(yr,8,5)),0.035*base[min(W-1,wk(D(yr,8,5)))]+120e3,6,0.7,"Semestre 2: novos campi" if yr==2027 else None)
    # sazonalidade: férias jan e jul
    for d0,amp in ((D(yr,1,1),-0.07),(D(yr,7,1),-0.04)):
        w=wk(d0)
        if 0<=w<W:
            k=np.arange(W)-w; m=(k>=0)&(k<8); y[m]*=1+amp*np.sin(np.pi*k[m]/8)
bump(wk(D(2027,10,15)),600e3,5,0.35,"Provas de concurso grande:\npico de grupos de estudo",1)
bump(wk(D(2028,5,10)),1.4e6,4,0.45,"Collab com criador(a)\ngrande (live + comunidade)",1)
bump(wk(D(2028,11,8)),1.0e6,3,0.4,"ENEM/concursos: bots de\nsimulado viralizam",1)
bump(wk(D(2029,6,20)),5.5e6,2,0.3,"Choque externo: apagão/bloqueio\nde app concorrente (cf. Telegram 2015,\nBluesky 2024 — retém ~30–50%)",1)
bump(wk(D(2030,3,1)),1.2e6,5,0.6,"Abertura geral\n(sai do nicho)",1)
noise=np.exp(np.convolve(rng.normal(0,0.035,W),np.ones(3)/3,"same")); y=y*noise
ylog=y/1e6
fig,ax=plt.subplots(figsize=(14,8),dpi=150)
ax.fill_between(days,0,base/1e6,color="#E8EEF9",label="Tendência de base, sem eventos (curva em S logística)")
ax.plot(days,base/1e6,"--",color="#7A8FB8",lw=1.3)
ax.plot(days,ylog,color="#5B3CC4",lw=2.4,label="MAU ilustrativo (com subidas e quedas)")
ax.set_ylabel("Usuários ativos por mês (milhões)"); ax.set_ylim(0,max(ylog)*1.18)
ax.xaxis.set_major_locator(mdates.MonthLocator(bymonth=(1,7))); ax.xaxis.set_major_formatter(mdates.DateFormatter("%b/%y"))
meses={"Jan":"jan","Jul":"jul"}
ax.xaxis.set_major_formatter(matplotlib.ticker.FuncFormatter(lambda v,_: ("jan/" if mdates.num2date(v).month==1 else "jul/")+mdates.num2date(v).strftime("%y")))
ax.grid(axis="y",alpha=.25)
# fases
phases=[(D(2027,3,1),D(2028,2,1),"Fase 1 · rede atômica\n(campi + grupos de concurso)"),(D(2028,2,1),D(2030,3,1),"Fase 2 · nicho → nichos vizinhos\n(gamers, criadores)"),(D(2030,3,1),days[-1],"Fase 3 · massa")]
cols=["#FFF4E0","#EAF7EE","#F3ECFF"]
for (a,b,lbl),c in zip(phases,cols):
    ax.axvspan(a,b,color=c,alpha=.6,zorder=0); ax.text(a+timedelta(days=20),max(ylog)*1.12,lbl,fontsize=9.5,va="top",color="#444",fontweight="bold")
pos={0:("2027-03-20",8.0),7:("2027-05-01",12.5),wk(D(2027,8,5)):("2027-07-15",17.0),wk(D(2027,10,15)):("2027-09-20",21.5),
     wk(D(2028,2,20)):("2027-12-20",8.0),wk(D(2028,5,10)):("2028-03-10",12.5),wk(D(2028,11,8)):("2028-12-10",1.0),
     wk(D(2029,6,20)):("2028-12-20",19.0),wk(D(2030,3,1)):("2029-09-15",27.0)}
for w,lbl,_ in ev:
    d,yt=pos[w]; yy=ylog[min(W-1,w)]
    ax.annotate(lbl,xy=(days[w],yy),xytext=(date.fromisoformat(d),yt),fontsize=8.6,ha="left",
        arrowprops=dict(arrowstyle="->",color="#5B3CC4",lw=1),bbox=dict(boxstyle="round,pad=0.25",fc="white",ec="#D0C8F0",alpha=.95))
wmc=wk(D(2028,9,15))
ax.annotate("Massa crítica (proxy): retenção estabiliza\ne o crescimento vem de convites, sem mídia paga",xy=(days[wmc],ylog[wmc]),xytext=(date(2028,2,1),25.5),fontsize=9,ha="left",fontweight="bold",color="#1D6B3A",
    arrowprops=dict(arrowstyle="->",color="#1D6B3A",lw=1.5),bbox=dict(boxstyle="round,pad=0.3",fc="#E9F8EE",ec="#1D6B3A"))
wcm=int(np.argmax(np.gradient(base)))
ax.annotate("Inflexão da curva em S\n(crescimento absoluto máximo)",xy=(days[wcm],base[wcm]/1e6),xytext=(date(2029,4,1),3.0),fontsize=8.6,ha="left",color="#4A5A80",
    arrowprops=dict(arrowstyle="->",color="#7A8FB8",lw=1),bbox=dict(boxstyle="round,pad=0.25",fc="white",ec="#7A8FB8"))
ax.legend(loc="upper left",bbox_to_anchor=(0.0,0.86),frameon=False,fontsize=10)
ax.set_title("Zoen — curva de crescimento ILUSTRATIVA para uma rede social nova no Brasil",fontsize=15,fontweight="bold",loc="left",pad=24)
ax.text(0,1.012,"MODELO ILUSTRATIVO, não são dados reais. Curva em S (teto 30 mi MAU) + eventos com retenção parcial calibrada nos casos reais (ver relatório).",transform=ax.transAxes,fontsize=9.5,color="#B03030")
# inset log
ins=fig.add_axes([0.72,0.13,0.17,0.19]); ins.plot(days,ylog*1e6,color="#5B3CC4",lw=1.2); ins.set_yscale("log"); ins.set_title("mesma curva, escala log",fontsize=8); ins.tick_params(labelsize=7)
ins.xaxis.set_major_locator(mdates.YearLocator()); ins.xaxis.set_major_formatter(mdates.DateFormatter("%Y"))
ins.yaxis.set_major_formatter(matplotlib.ticker.FuncFormatter(lambda v,_: f"{v/1e6:g} mi"))
fig.tight_layout(); fig.savefig("docs/research/img/zoen-curva-crescimento.png"); plt.close(fig)
print("ok", days[wmc], round(ylog[wmc],2)); print("ok", round(ylog.max(),1), days[wcm], [ (days[w],round(ylog[w],2)) for w in (0,3,7,26,52,104,156)])
