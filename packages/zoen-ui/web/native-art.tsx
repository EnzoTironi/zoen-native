import React, { useEffect, useId, useState } from 'react';
import { isDisplayClockFixed } from './display-clock';
import './native-art.css';

// Project-owned AvatarV1Manifest.json, in native catalog order. Picking mirrors
// HandDrawnAvatarAsset.swift; no title rules beyond those in the native source.
const catalog = [{"id":"agent-furball-wave","kind":"agent","name":"AgentFurballWave","tags":["mascot","assistant","general"],"backdrop":"#D9825B","still":"AgentFurballWave","hero":"Heroes/AgentFurballWave.webp"},{"id":"agent-furball-reader","kind":"agent","name":"AgentFurballReader","tags":["mascot","research","reading"],"backdrop":"#8DB7D4","still":"AgentFurballReader","hero":"Heroes/AgentFurballReader.webp"},{"id":"agent-furball-headphones","kind":"agent","name":"AgentFurballHeadphones","tags":["mascot","music","podcast"],"backdrop":"#E5A4A0","still":"AgentFurballHeadphones","hero":"Heroes/AgentFurballHeadphones.webp"},{"id":"agent-robot-chef","kind":"agent","name":"AgentRobotChef","tags":["chef","recipes","cooking"],"backdrop":"#9B7FB0","still":"AgentRobotChef","hero":"Heroes/AgentRobotChef.webp"},{"id":"agent-owl-tutor","kind":"agent","name":"AgentOwlTutor","tags":["tutor","school","learning"],"backdrop":"#A8C3A0","still":"AgentOwlTutor","hero":"Heroes/AgentOwlTutor.webp"},{"id":"agent-fox-traveler","kind":"agent","name":"AgentFoxTraveler","tags":["travel","trips","planner"],"backdrop":"#A8C3A0","still":"AgentFoxTraveler","hero":"Heroes/AgentFoxTraveler.webp"},{"id":"agent-octopus-coder","kind":"agent","name":"AgentOctopusCoder","tags":["coding","developer","tech"],"backdrop":"#6F9FA8","still":"AgentOctopusCoder","hero":"Heroes/AgentOctopusCoder.webp"},{"id":"agent-piggy-bank","kind":"agent","name":"AgentPiggyBank","tags":["finance","budget","money"],"backdrop":"#A8C3A0","still":"AgentPiggyBank","hero":"Heroes/AgentPiggyBank.webp"},{"id":"agent-frog-fitness","kind":"agent","name":"AgentFrogFitness","tags":["fitness","workout","health"],"backdrop":"#6F9FA8","still":"AgentFrogFitness","hero":"Heroes/AgentFrogFitness.webp"},{"id":"agent-calendar-buddy","kind":"agent","name":"AgentCalendarBuddy","tags":["calendar","schedule","planning"],"backdrop":"#A8C3A0","still":"AgentCalendarBuddy","hero":"Heroes/AgentCalendarBuddy.webp"},{"id":"agent-coffee-cup","kind":"agent","name":"AgentCoffeeCup","tags":["assistant","morning","daily"],"backdrop":"#8DB7D4","still":"AgentCoffeeCup","hero":"Heroes/AgentCoffeeCup.webp"},{"id":"agent-cat-detective","kind":"agent","name":"AgentCatDetective","tags":["research","search","facts"],"backdrop":"#9B7FB0","still":"AgentCatDetective","hero":"Heroes/AgentCatDetective.webp"},{"id":"agent-potted-plant","kind":"agent","name":"AgentPottedPlant","tags":["wellness","garden","habits"],"backdrop":"#A8C3A0","still":"AgentPottedPlant","hero":"Heroes/AgentPottedPlant.webp"},{"id":"agent-turtle-writer","kind":"agent","name":"AgentTurtleWriter","tags":["writing","editor","notes"],"backdrop":"#E5A4A0","still":"AgentTurtleWriter","hero":"Heroes/AgentTurtleWriter.webp"},{"id":"agent-bee-tasks","kind":"agent","name":"AgentBeeTasks","tags":["tasks","productivity","todo"],"backdrop":"#8DB7D4","still":"AgentBeeTasks","hero":"Heroes/AgentBeeTasks.webp"},{"id":"agent-penguin-weather","kind":"agent","name":"AgentPenguinWeather","tags":["weather","forecast"],"backdrop":"#8DB7D4","still":"AgentPenguinWeather","hero":"Heroes/AgentPenguinWeather.webp"},{"id":"agent-lightbulb-ideas","kind":"agent","name":"AgentLightbulbIdeas","tags":["ideas","brainstorm","creative"],"backdrop":"#E5A4A0","still":"AgentLightbulbIdeas","hero":"Heroes/AgentLightbulbIdeas.webp"},{"id":"agent-rocket-news","kind":"agent","name":"AgentRocketNews","tags":["news","launch","updates"],"backdrop":"#8DB7D4","still":"AgentRocketNews","hero":"Heroes/AgentRocketNews.webp"},{"id":"agent-dog-courier","kind":"agent","name":"AgentDogCourier","tags":["email","messages","inbox"],"backdrop":"#A8C3A0","still":"AgentDogCourier","hero":"Heroes/AgentDogCourier.webp"},{"id":"agent-teapot-calm","kind":"agent","name":"AgentTeapotCalm","tags":["meditation","calm","mindfulness"],"backdrop":"#7FAF6B","still":"AgentTeapotCalm","hero":"Heroes/AgentTeapotCalm.webp"},{"id":"agent-alarm-clock","kind":"agent","name":"AgentAlarmClock","tags":["reminders","time","alarms"],"backdrop":"#A8C3A0","still":"AgentAlarmClock","hero":"Heroes/AgentAlarmClock.webp"},{"id":"agent-bear-doctor","kind":"agent","name":"AgentBearDoctor","tags":["health","doctor","care"],"backdrop":"#9B7FB0","still":"AgentBearDoctor","hero":"Heroes/AgentBearDoctor.webp"},{"id":"agent-hedgehog-shopper","kind":"agent","name":"AgentHedgehogShopper","tags":["shopping","deals","groceries"],"backdrop":"#7FAF6B","still":"AgentHedgehogShopper","hero":"Heroes/AgentHedgehogShopper.webp"},{"id":"agent-moon-sleep","kind":"agent","name":"AgentMoonSleep","tags":["sleep","night","bedtime"],"backdrop":"#9B7FB0","still":"AgentMoonSleep","hero":"Heroes/AgentMoonSleep.webp"},{"id":"group-beach-trip","kind":"group","name":"GroupBeachTrip","tags":["trip","beach","vacation","summer"],"backdrop":"#7FAF6B","still":"GroupBeachTrip","hero":"Heroes/GroupBeachTrip.webp"},{"id":"group-soccer-ball","kind":"group","name":"GroupSoccerBall","tags":["soccer","football","sports","team"],"backdrop":"#9B7FB0","still":"GroupSoccerBall","hero":"Heroes/GroupSoccerBall.webp"},{"id":"group-family-dinner","kind":"group","name":"GroupFamilyDinner","tags":["family","dinner","home"],"backdrop":"#8DB7D4","still":"GroupFamilyDinner","hero":"Heroes/GroupFamilyDinner.webp"},{"id":"group-study-desk","kind":"group","name":"GroupStudyDesk","tags":["study","school","university"],"backdrop":"#A8C3A0","still":"GroupStudyDesk","hero":"Heroes/GroupStudyDesk.webp"},{"id":"group-guitar-music","kind":"group","name":"GroupGuitarMusic","tags":["music","band","guitar"],"backdrop":"#7FAF6B","still":"GroupGuitarMusic","hero":"Heroes/GroupGuitarMusic.webp"},{"id":"group-work-laptop","kind":"group","name":"GroupWorkLaptop","tags":["work","team","office"],"backdrop":"#E3B55B","still":"GroupWorkLaptop","hero":"Heroes/GroupWorkLaptop.webp"},{"id":"group-camping-tent","kind":"group","name":"GroupCampingTent","tags":["camping","outdoors","friends"],"backdrop":"#8DB7D4","still":"GroupCampingTent","hero":"Heroes/GroupCampingTent.webp"},{"id":"group-party-balloons","kind":"group","name":"GroupPartyBalloons","tags":["party","celebration","friends"],"backdrop":"#9B7FB0","still":"GroupPartyBalloons","hero":"Heroes/GroupPartyBalloons.webp"},{"id":"group-book-club","kind":"group","name":"GroupBookClub","tags":["books","reading","club"],"backdrop":"#9B7FB0","still":"GroupBookClub","hero":"Heroes/GroupBookClub.webp"},{"id":"group-gym","kind":"group","name":"GroupGym","tags":["gym","fitness","training"],"backdrop":"#9B7FB0","still":"GroupGym","hero":"Heroes/GroupGym.webp"},{"id":"group-pets","kind":"group","name":"GroupPets","tags":["pets","dogs","cats"],"backdrop":"#7FAF6B","still":"GroupPets","hero":"Heroes/GroupPets.webp"},{"id":"group-city-skyline","kind":"group","name":"GroupCitySkyline","tags":["city","neighborhood","local"],"backdrop":"#6F9FA8","still":"GroupCitySkyline","hero":"Heroes/GroupCitySkyline.webp"},{"id":"group-houseplants","kind":"group","name":"GroupHouseplants","tags":["plants","garden","home"],"backdrop":"#9B7FB0","still":"GroupHouseplants","hero":"Heroes/GroupHouseplants.webp"},{"id":"group-board-games","kind":"group","name":"GroupBoardGames","tags":["games","board games","fun"],"backdrop":"#8DB7D4","still":"GroupBoardGames","hero":"Heroes/GroupBoardGames.webp"},{"id":"group-cooking","kind":"group","name":"GroupCooking","tags":["cooking","recipes","food"],"backdrop":"#9B7FB0","still":"GroupCooking","hero":"Heroes/GroupCooking.webp"},{"id":"group-road-trip","kind":"group","name":"GroupRoadTrip","tags":["road trip","travel","friends"],"backdrop":"#7FAF6B","still":"GroupRoadTrip","hero":"Heroes/GroupRoadTrip.webp"},{"id":"group-mountain-hike","kind":"group","name":"GroupMountainHike","tags":["hiking","mountains","outdoors"],"backdrop":"#E5A4A0","still":"GroupMountainHike","hero":"Heroes/GroupMountainHike.webp"},{"id":"group-coffee-friends","kind":"group","name":"GroupCoffeeFriends","tags":["friends","coffee","chat"],"backdrop":"#A8C3A0","still":"GroupCoffeeFriends","hero":"Heroes/GroupCoffeeFriends.webp"},{"id":"group-picnic","kind":"group","name":"GroupPicnic","tags":["picnic","weekend","friends"],"backdrop":"#9B7FB0","still":"GroupPicnic","hero":"Heroes/GroupPicnic.webp"},{"id":"group-birthday-cake","kind":"group","name":"GroupBirthdayCake","tags":["birthday","celebration","family"],"backdrop":"#E3B55B","still":"GroupBirthdayCake","hero":"Heroes/GroupBirthdayCake.webp"},{"id":"group-bike-ride","kind":"group","name":"GroupBikeRide","tags":["cycling","bike","sports"],"backdrop":"#A8C3A0","still":"GroupBikeRide","hero":"Heroes/GroupBikeRide.webp"},{"id":"group-video-games","kind":"group","name":"GroupVideoGames","tags":["gaming","video games","friends"],"backdrop":"#9B7FB0","still":"GroupVideoGames","hero":"Heroes/GroupVideoGames.webp"}] as const;
const groupRules: readonly (readonly [readonly string[], readonly string[]])[] = [[["viajantes","litoral","coastal","praia","beach"],["beach","trip","vacation","summer"]],[["hike","trail","trilha","turma","sábado","saturday","crew","pedra"],["hiking","mountains","outdoors"]],[["paraty","feriado","viagem"],["road trip","travel"]],[["produto","product","work","trabalho","office","zoen ·"],["work","team","office","laptop"]],[["jantar","dinner","cook","food"],["cooking","food","dinner"]],[["music","música","band"],["music","band"]],[["pet","dog","cat","paçoca"],["pets"]],[["work","trabalho","office"],["work","team","office"]],[["study","estudo","school"],["study","school"]],[["party","festa"],["party","celebration"]],[["game","jogo"],["games","gaming"]],[["bike","pedal"],["cycling","bike"]],[["gym","treino"],["gym","fitness"]],[["book","livro"],["books","reading"]],[["camp","acamp"],["camping","outdoors"]],[["picnic"],["picnic","weekend"]],[["road"],["road trip","travel"]],[["city","cidade"],["city","neighborhood"]],[["plant","jardim"],["plants","garden"]],[["coffee","café"],["coffee","friends"]],[["family","família"],["family","dinner"]],[["soccer","futebol"],["soccer","sports"]],[["birth","anivers"],["birthday","celebration"]]];
export type NativeAvatarIdentity = {
  kind: 'person' | 'group' | 'agent'; id?: string; name: string; handle?: string;
  initials?: string; tintHex?: string; avatarRef?: string; photoUrl?: string; hintTags?: string[];
};

export function nativeAvatarHash(seed: string): number {
  let hash = 2166136261;
  for (const scalar of seed) hash = Math.imul(hash ^ scalar.codePointAt(0)!, 16777619) >>> 0;
  return hash % 9973;
}

export function nativeAvatarAsset(identity: NativeAvatarIdentity) {
  if (identity.kind === 'person') return undefined;
  if (identity.avatarRef) {
    const selected = catalog.find(a => a.id === identity.avatarRef || a.name === identity.avatarRef || a.still === identity.avatarRef);
    if (selected) return selected;
  }
  const pool = catalog.filter(a => a.kind === identity.kind);
  const tags = [...(identity.hintTags ?? [])];
  if (identity.kind === 'group') {
    const title = identity.name.toLowerCase();
    const rule = groupRules.find(([keys]) => keys.some(key => title.includes(key)));
    if (rule) tags.push(...rule[1]);
  } else {
    const aliases: Record<string, string[]> = {
      zoen: ['mascot', 'assistant', 'general'],
      financeiro: ['finance', 'budget', 'money'], finance: ['finance', 'budget', 'money'],
      organizador: ['tasks', 'productivity', 'todo', 'calendar'], organizer: ['tasks', 'productivity', 'todo', 'calendar'],
      guia: ['travel', 'trips', 'planner'], guide: ['travel', 'trips', 'planner'],
    };
    tags.push(...(aliases[(identity.handle ?? '').toLowerCase()] ?? []));
  }
  const needles = tags.map(tag => tag.toLowerCase()).filter(Boolean);
  const scored = pool.map(asset => ({ asset, score: asset.tags.filter(tag => needles.some(n => n === tag || n.includes(tag) || tag.includes(n))).length })).sort((a, b) => b.score - a.score);
  if (scored[0]?.score) return scored[0].asset;
  const seed = identity.id || (identity.kind === 'agent' ? identity.handle ?? '' : identity.name);
  return pool[nativeAvatarHash(seed) % pool.length];
}

/** Host passes persisted avatarRef; native UserDefaults cannot be inferred on web.
 * People retain photo/monogram fallback. Small avatars use native catalog stills;
 * large avatars use native loops (decoded frame zero while the display clock is fixed).
 * Zoen's procedural animated head remains a separate native-renderer parity gap.
 */
export function nativeAvatarSource(identity: NativeAvatarIdentity, size: number, { animate = true, reduceMotion = false, fixed = isDisplayClockFixed() } = {}): string | undefined {
  if (identity.kind === 'person') return identity.photoUrl;
  if (identity.kind === 'agent' && identity.handle?.toLowerCase() === 'zoen') return './assets/native/head.png';
  const asset = nativeAvatarAsset(identity);
  if (!asset) return undefined;
  const loop = animate && !reduceMotion && size >= 56;
  return './assets/avatars/' + (loop ? fixed ? `Frame0/${asset.name}.png` : `Loops/${asset.name}.webp` : `Stills/${asset.still}.png`);
}

function useReduceMotion() {
  const [reduced, setReduced] = useState(() => typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches);
  useEffect(() => {
    if (typeof matchMedia === 'undefined') return;
    const query = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setReduced(query.matches);
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  }, []);
  return reduced;
}

export function NativeAvatar({ identity, size = 48, className = '', animate = true }: {
  identity: NativeAvatarIdentity; size?: number; className?: string; animate?: boolean;
}) {
  const reduced = useReduceMotion();
  const filterId = 'native-avatar-' + useId().replace(/:/g, '');
  const proceduralHead = identity.kind === 'agent' && identity.handle?.toLowerCase() === 'zoen';
  const asset = proceduralHead ? undefined : nativeAvatarAsset(identity);
  const src = nativeAvatarSource(identity, size, { animate, reduceMotion: reduced });
  return <span className={'native-avatar ' + (identity.kind === 'person' && !src ? 'native-avatar-monogram ' : '') + className} role="img" aria-label={identity.name} style={{ width: size, height: size, borderRadius: '50%', display: 'inline-grid', placeItems: 'center', overflow: 'hidden', flexShrink: 0, backgroundColor: proceduralHead ? '#E2E8CF' : asset?.backdrop ?? identity.tintHex ?? '#888888', color: 'white', fontSize: size * (identity.kind === 'person' ? .4 : .38), ...(identity.kind === 'person' && !src ? { '--native-avatar-tint': identity.tintHex ?? '#888888' } as React.CSSProperties : {}) }}>
    {asset && <svg width="0" height="0" aria-hidden="true" style={{ position: 'absolute' }}><defs><filter id={filterId} colorInterpolationFilters="sRGB"><feColorMatrix type="matrix" values=".86 0 0 0 0  0 .84 0 0 0  0 0 .80 0 0  0 0 0 1 0"/></filter></defs></svg>}
    {src ? <img className={asset ? 'native-avatar-art' : undefined} src={src} alt="" width={size} height={size} style={{ width: proceduralHead ? '108%' : '100%', height: proceduralHead ? '108%' : '100%', objectFit: 'cover', ...(proceduralHead ? { transform: `translateY(${size * .05}px)` } : {}), ...(asset ? { '--native-avatar-dark-filter': `url(#${filterId})` } as React.CSSProperties : {}) }}/> : identity.initials ?? identity.name.slice(0, 1)}
  </span>;
}

/** Exact native Ink export, frozen at t=2.4; not a re-drawn SVG or animation. */
export function NativeWelcomeMascot({ src = './assets/native/wave.png' }: { src?: string | undefined }) {
  return <img className="no-mascot" src={src} alt="Zoen waving" width="230" height="230"/>;
}
