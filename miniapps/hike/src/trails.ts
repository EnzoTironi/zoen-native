// Static trail data that ships in the bundle: routes, photos and copy. The shared, live
// part (votes, decision, itinerary, album) comes from the core as state.
export type TrailId = 'tomales' | 'steep' | 'lands';

export interface Trail {
  id: TrailId;
  name: string;
  park: string;
  miles: number;
  climbFt: number;
  time: string;
  difficulty: 'Easy' | 'Moderate' | 'Hard';
  route: 'Out & back' | 'Loop';
  terrain: string;
  shade: string;
  bestFor: string;
  about: string;
  attributes: string[];
  trailhead: [number, number]; // lon, lat
  line: [number, number][];
  link: string;
}

export const TRAILS: Trail[] = [
  {
    id: 'tomales', name: 'Tomales Point', park: 'Point Reyes National Seashore', miles: 9.5, climbFt: 1100, time: '4–5 hr',
    difficulty: 'Moderate', route: 'Out & back', terrain: 'Coastal bluffs', shade: 'None', bestFor: 'Tule elk, ocean views',
    about: 'Follows the ridge from Pierce Point Ranch out to the tip of the peninsula, with the Pacific on one side and Tomales Bay on the other. Tule elk graze right next to the trail; the last mile turns to sand.',
    attributes: ['Wildlife', 'Ocean views', 'Windy', 'No dogs'],
    trailhead: [-122.9553, 38.1893],
    line: [[-122.9553, 38.1893], [-122.958, 38.196], [-122.9625, 38.203], [-122.9655, 38.21], [-122.97, 38.217], [-122.976, 38.224], [-122.981, 38.23], [-122.986, 38.236], [-122.991, 38.241], [-122.995, 38.244]],
    link: 'https://www.nps.gov/pore/planyourvisit/hiking.htm',
  },
  {
    id: 'steep', name: 'Steep Ravine', park: 'Mount Tamalpais State Park', miles: 3.8, climbFt: 1100, time: '2–3 hr',
    difficulty: 'Moderate', route: 'Loop', terrain: 'Redwood canyon', shade: 'Full', bestFor: 'Waterfalls, hot days',
    about: 'Drops from Pantoll through a cool redwood canyon along Webb Creek, over footbridges and up a ten-foot wooden ladder beside a waterfall, then climbs back on the Dipsea.',
    attributes: ['Redwoods', 'Waterfall', 'Ladder', 'Shady'],
    trailhead: [-122.604, 37.9038],
    line: [[-122.604, 37.9038], [-122.6075, 37.9025], [-122.611, 37.901], [-122.615, 37.8995], [-122.619, 37.8985], [-122.623, 37.8975], [-122.6262, 37.8968], [-122.6215, 37.899], [-122.616, 37.9008], [-122.61, 37.903], [-122.604, 37.9038]],
    link: 'https://www.parks.ca.gov/?page_id=471',
  },
  {
    id: 'lands', name: 'Lands End', park: 'Golden Gate National Recreation Area', miles: 3.4, climbFt: 550, time: '1.5–2 hr',
    difficulty: 'Easy', route: 'Out & back', terrain: 'Cypress cliffs', shade: 'Partial', bestFor: 'Golden Gate views, short day',
    about: 'A cliff-top path through wind-bent cypress from the Sutro Baths ruins to Eagle\u2019s Point, with the Golden Gate Bridge opening up at every bend and a stone labyrinth on the way.',
    attributes: ['City views', 'Bridge', 'Labyrinth', 'Kid friendly'],
    trailhead: [-122.5112, 37.7803],
    line: [[-122.5112, 37.7803], [-122.5085, 37.7818], [-122.5052, 37.7838], [-122.502, 37.7855], [-122.499, 37.7868], [-122.496, 37.7878], [-122.4935, 37.788], [-122.4925, 37.7875]],
    link: 'https://www.nps.gov/goga/planyourvisit/landsend.htm',
  },
];

export const byId = (id: string) => TRAILS.find((t) => t.id === id)!;

export function haversineMi(a: [number, number], b: [number, number]) {
  const R = 3958.8, toR = Math.PI / 180;
  const dLat = (b[1] - a[1]) * toR, dLon = (b[0] - a[0]) * toR;
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(a[1] * toR) * Math.cos(b[1] * toR) * Math.sin(dLon / 2) ** 2;
  return 2 * R * Math.asin(Math.sqrt(h));
}

export function bounds(line: [number, number][]): [[number, number], [number, number]] {
  const xs = line.map((p) => p[0]), ys = line.map((p) => p[1]);
  return [[Math.min(...xs), Math.min(...ys)], [Math.max(...xs), Math.max(...ys)]];
}
