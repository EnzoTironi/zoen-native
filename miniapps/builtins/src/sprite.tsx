const sideRows = [
  '................OO.OO.....',
  '...............OPO.OPO....',
  '...............OPO.OPO....',
  '...............OPOOOPO....',
  '..............OMGGGGGGO...',
  '.............OMMGGGGGGGO..',
  '.............OMGGGEEGGGGO.',
  '............OMMGGGEHGGGGO.',
  '............OMGGGGGGGGWWWO',
  '...........OMMGGGGGGWWWWWO',
  '...........OMGGGGGGWWWNWWO',
  '..OOOOOOOOOMMGGGGGGOWWWWO.',
  '.OGGGGGGGGGGGGGGGGGGOOOO..',
  'OGGGGGGGGGGGGGGGGGGGO.....',
  'OGGGGGGGGGGGGGGGGGGGO.....',
  'TOGGGGGGGGGGGGGGGGGGO.....',
  'T.OGGWWWWWWWWWWWGGGO......',
  'T..OGGGGGGGGGGGGGGO.......',
];
const legs = [
  '...OGO..OGO....OGO..OGO...',
  '...OGO..OGO....OGO..OGO...',
  '...OGO..OGO....OGO..OGO...',
  '...OKO..OKO....OKO..OKO...',
];
const palette: Record<string, string> = {
  O: '#3A3340',
  G: '#9C97A3',
  M: '#4B4452',
  P: '#F4A3A8',
  W: '#F3EEE8',
  E: '#17131C',
  H: '#FFFFFF',
  N: '#6E6673',
  K: '#2C2730',
  T: '#4B4452',
};
const awake = [...sideRows, ...legs];
const closedEyes = sideRows.map((row, index) =>
  index === 6 ? row.replaceAll('E', 'G') : index === 7 ? row.replace(/[EH]/g, 'O') : row,
);
const sleeping = [
  '.'.repeat(26),
  '.'.repeat(26),
  '.'.repeat(26),
  ...closedEyes,
  '...OOOO.........OOOO......',
];
/** Current native DonkeyArt (26×22), self-contained without asset requests. */
export function Sprite({
  x = 0,
  y = 0,
  scale = 1,
  asleep = false,
}: {
  x?: number;
  y?: number;
  scale?: number;
  asleep?: boolean;
}) {
  return (
    <g transform={`translate(${x} ${y}) scale(${scale})`}>
      {(asleep ? sleeping : awake).flatMap((row, j) =>
        [...row].flatMap((color, i) =>
          color === '.'
            ? []
            : [<rect key={`${i}:${j}`} x={i} y={j} width={1} height={1} fill={palette[color]} />],
        ),
      )}
    </g>
  );
}
