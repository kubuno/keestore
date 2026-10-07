interface KeeStoreLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Keestore logo (designer artwork, raster). Served by the host from
 *  `/keestore-logo.png`; rendered as a square image so it weighs the same as
 *  its neighbours in the waffle menu. */
export function KeeStoreLogo({ size = 24, className, title = 'Keestore' }: KeeStoreLogoProps) {
  return (
    <img
      src="/keestore-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default KeeStoreLogo
