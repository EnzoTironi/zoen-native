# Zoen unit economics calculator. Prices: vendor pages fetched 2026-10-09.
models = {  # name: (input $/M, cached input $/M, output $/M)
 "GPT-6 Luna (OpenAI)": (0.10, 0.01, 0.50),
 "GPT-5 nano (OpenAI)": (0.05, 0.005, 0.40),
 "Claude Haiku 5.5 (Anthropic, <=100k)": (0.10, 0.01, 0.50),
 "Gemini 2.5 Flash-Lite (Google)": (0.10, 0.01, 0.40),
 "Gemini 3.1 Flash-Lite (Google)": (0.25, 0.025, 1.50),
 "Gemini 3.5 Flash-Lite (Google)": (0.30, 0.03, 2.50),
 "DeepSeek Flash V4.1 (pico)": (0.30, 0.006, 1.20),
 "DeepSeek Flash V4.1 (fora de pico)": (0.15, 0.003, 0.60),
 "Gemini 3.8 Flash (promo ate 31/12/2026)": (0.75, 0.075, 3.75),
 "Gemini 3.8 Flash (a partir de 1/1/2027)": (1.50, 0.15, 7.50),
 "Grok build-0.1 (xAI, mais barato listado)": (1.00, 0.20, 2.00),
}
shapes = {"leve 1.5k/300": (1500, 300), "pesada 4k/800": (4000, 800)}
CACHE_SHARE = 0.7  # assumption: 70% of input is a reusable system/tool prefix
def call_cost(m, s, cached):
    i, c, o = models[m]; tin, tout = shapes[s]
    if cached:
        return (tin*(1-CACHE_SHARE)*i + tin*CACHE_SHARE*c + tout*o)/1e6
    return (tin*i + tout*o)/1e6
print("| modelo | chamada leve | leve c/ cache | chamada pesada | pesada c/ cache | 5/mês leve | 5/dia leve (150/mês) | 5/dia pesada |")
print("|---|---|---|---|---|---|---|---|")
for m in models:
    l=call_cost(m,"leve 1.5k/300",False); lc=call_cost(m,"leve 1.5k/300",True)
    h=call_cost(m,"pesada 4k/800",False); hc=call_cost(m,"pesada 4k/800",True)
    print(f"| {m} | ${l:.5f} | ${lc:.5f} | ${h:.5f} | ${hc:.5f} | ${5*lc:.4f} | ${150*lc:.3f} | ${150*hc:.3f} |")

# Ad revenue per MAU per month
print()
scen = {  # share in communities, active days/mo, community sessions/day, banners/session, fill, CPM USD
 "baixo": (0.40, 15, 1, 1, 0.30, 0.10),
 "médio": (0.60, 20, 3, 2, 0.60, 0.50),
 "alto":  (0.80, 25, 6, 3, 0.90, 2.00),
}
for k,(sh,d,s,b,f,cpm) in scen.items():
    imp = sh*d*s*b*f
    gross = imp*cpm/1000
    print(f"{k}: impressões/MAU/mês={imp:.1f} bruto=${gross:.4f} líquido(50% p/ dono da comunidade)=${gross/2:.4f}")
# global mix: 70% BR + 30% US-like at 4.5x CPM (Appodeal NA/LATAM banner ~4-5x; Reddit US/intl ARPU 5.2x)
for k,(sh,d,s,b,f,cpm) in scen.items():
    imp=sh*d*s*b*f; g=imp*cpm*(0.7+0.3*4.5)/1000
    print(f"global-mix {k}: bruto=${g:.4f} líquido=${g/2:.4f}")

# break-even impressions per MAU per month
print()
for cost in (0.005, 0.06, 0.92, 1.82):
    row=[]
    for cpm in (0.10,0.50,1.00,2.00,5.00):
        row.append(f"{cost/(cpm/1000):.0f} ({cost/(cpm/1000)/30:.1f}/dia)")
    print(cost, row)
# benchmarks
print("Telegram ads/MAU/mo", 125e6/6/1e9, "total", 870e6/6/1e9, "FY24 total", 1.4e9/12/1e9)
print("Reddit intl per DAUq/mo", 2.26/3, "per WAUq/mo", 166.8/317.4/3, "US per DAU/mo", 11.85/3)
print("Meta ARPP/mo", 16.86/3, "FB RoW 2023 /mo", 4.50/3)
print("Discord rev/MAU/mo", 561/250/12)
print("Signal infra/MAU/mo @40M", 14e6/12/40e6, "ex-registration", 8e6/12/40e6)
print("BRL/USD implied", 0.3217/0.0625, "Meta BR CPM USD", [x/(0.3217/0.0625) for x in (8,15,35)])
# commissions
for k,(pen,spend,take) in {"baixo":(0.01,2,0.10),"médio":(0.03,4,0.15),"alto":(0.05,8,0.20)}.items():
    print("loja",k,pen*spend*take)
for k,(per1k,arpa) in {"baixo":(2,5),"médio":(5,10),"alto":(10,20)}.items():
    print("bots empresa",k,per1k/1000*arpa)
for k,(msgs,price) in {"baixo":(2,0.002),"médio":(4,0.005),"alto":(8,0.0068)}.items():
    print("msg empresa",k,msgs*price)
