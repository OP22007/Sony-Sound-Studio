export type ProductArtwork = {
  src: string;
  kind: 'over-ear' | 'earbud' | 'neckband';
  alt: string;
  width: number;
  height: number;
  presentation?: 'scene';
};

export const productArtwork: Record<string, ProductArtwork> = {
  'WH-1000XX': { src: '/headphones/sony-1000x-collexion-black.png', kind: 'over-ear', alt: '1000X THE COLLEXION official Sony black product render', width: 1200, height: 848 },
  '1000X THE COLLEXION': { src: '/headphones/sony-1000x-collexion-black.png', kind: 'over-ear', alt: '1000X THE COLLEXION official Sony black product render', width: 1200, height: 848 },
  'WH-1000XM6': { src: '/headphones/wh-1000xm6-black.png', kind: 'over-ear', alt: 'WH-1000XM6 official Sony product render', width: 1200, height: 1200 },
  'WH-1000XM5': { src: '/headphones/wh-1000xm5-black.png', kind: 'over-ear', alt: 'WH-1000XM5 official Sony product render', width: 1200, height: 1200 },
  'WH-CH720N': { src: '/headphones/wh-ch720n-black.png', kind: 'over-ear', alt: 'WH-CH720N official Sony product render', width: 1200, height: 821 },
  'ONEPLUS NORD BUDS 3 PRO': { src: '/headphones/oneplus-nord-buds-3-pro-mint.png', kind: 'earbud', alt: 'OnePlus Nord Buds 3 Pro mint earbud', width: 2187, height: 1680 },
  'ONEPLUS BUDS PRO 3': { src: '/headphones/oneplus-buds-pro-3-blue.png', kind: 'earbud', alt: 'OnePlus Buds Pro 3 blue earbuds and charging case', width: 1000, height: 751 },
  'ONEPLUS BUDS 3': { src: '/headphones/oneplus-buds-3.jpg', kind: 'earbud', alt: 'OnePlus Buds 3 earbuds and charging case', width: 1260, height: 630, presentation: 'scene' },
  'ONEPLUS BUDS PRO 2': { src: '/headphones/oneplus-buds-pro-2.jpg', kind: 'earbud', alt: 'OnePlus Buds Pro 2 earbuds and charging cases', width: 1200, height: 630, presentation: 'scene' },
  'ONEPLUS NORD BUDS 2R': { src: '/headphones/oneplus-nord-buds-2r.jpg', kind: 'earbud', alt: 'OnePlus Nord Buds 2r earbuds and cases', width: 2400, height: 1260, presentation: 'scene' },
  'ONEPLUS NORD BUDS 2': { src: '/headphones/oneplus-nord-buds-2.jpg', kind: 'earbud', alt: 'OnePlus Nord Buds 2 earbuds and charging cases', width: 1200, height: 630, presentation: 'scene' },
  'ONEPLUS NORD BUDS 3': { src: '/headphones/oneplus-nord-buds-3.jpg', kind: 'earbud', alt: 'OnePlus Nord Buds 3 earbuds and charging cases', width: 1200, height: 630, presentation: 'scene' },
  'ONEPLUS BUDS Z2': { src: '/headphones/oneplus-buds-z2.jpg', kind: 'earbud', alt: 'OnePlus Buds Z2 product render', width: 1200, height: 630, presentation: 'scene' },
  'ONEPLUS BUDS PRO': { src: '/headphones/oneplus-buds-pro.jpg', kind: 'earbud', alt: 'OnePlus Buds Pro product render', width: 800, height: 1360, presentation: 'scene' },
  'ONEPLUS BULLETSWIRELESS Z2 ANC': { src: '/headphones/oneplus-bullets-wireless-z2-anc.jpg', kind: 'neckband', alt: 'OnePlus Bullets Wireless Z2 ANC neckband earphones', width: 2400, height: 1260, presentation: 'scene' },
  'ONEPLUS BULLETS WIRELESS Z3': { src: '/headphones/oneplus-bullets-wireless-z3.webp', kind: 'neckband', alt: 'OnePlus Bullets Wireless Z3 neckband earphones', width: 4320, height: 2160, presentation: 'scene' },
};
