import { isOnePlusModel } from './oneplus';
import { productArtwork } from './headphoneArtwork';

export default function Headphones({ compact = false, model = 'WH-1000XM6' }: { compact?: boolean; model?: string }) {
  const normalizedModel = model.trim().toUpperCase();
  const knownModel = isOnePlusModel(model)
    ? Object.keys(productArtwork).find(key => key.startsWith('ONEPLUS ') && normalizedModel === key)
    : Object.keys(productArtwork).find(key => !key.startsWith('ONEPLUS ') && normalizedModel.includes(key));
  if (!knownModel) return <div className="headphones art-unavailable">{isOnePlusModel(model) ? 'OnePlus product artwork unavailable' : 'Product artwork unavailable'}</div>;

  const artwork = productArtwork[knownModel];
  const classes = [
    'headphones',
    'product-render',
    compact ? 'compact' : '',
    artwork.kind === 'earbud' ? 'oneplus-earbud-render' : '',
    artwork.kind === 'neckband' ? 'oneplus-neckband-render' : '',
    artwork.presentation === 'scene' ? 'product-scene' : '',
    knownModel === 'WH-CH720N' ? 'ch720-render' : '',
  ].filter(Boolean).join(' ');

  return <img
    className={classes}
    src={artwork.src}
    alt={artwork.alt}
    width={artwork.width}
    height={artwork.height}
    style={artwork.presentation === 'scene' ? { rotate: '0deg', filter: 'none' } : undefined}
    draggable={false}
    decoding="async"
  />;
}
