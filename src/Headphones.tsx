const productArt = {
  'WH-1000XM6': '/headphones/wh-1000xm6-black.png',
  'WH-1000XM5': '/headphones/wh-1000xm5-black.png',
  'WH-CH720N': '/headphones/wh-ch720n-black.png',
};

export default function Headphones({ compact = false, model = 'WH-1000XM6' }: { compact?: boolean; model?: string }) {
  const knownModel = (Object.keys(productArt) as (keyof typeof productArt)[]).find(key => model.toUpperCase().includes(key));
  if (!knownModel) return <div className="headphones art-unavailable">Product artwork unavailable</div>;
  return <img
    className={`headphones product-render ${compact ? 'compact' : ''} ${knownModel === 'WH-CH720N' ? 'ch720-render' : ''}`}
    src={productArt[knownModel]}
    alt={`${knownModel} official Sony product render`}
    width={1200}
    height={knownModel === 'WH-CH720N' ? 821 : 1200}
    draggable={false}
    decoding="async"
  />;
}
