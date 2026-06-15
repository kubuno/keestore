interface KeeStoreLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Logo KeeStore : bouclier indigo + cadenas/clé blanc. Couleurs de marque fixes. */
export function KeeStoreLogo({ size = 24, className, title = 'Keestore' }: KeeStoreLogoProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 1180 1180"
      role="img"
      aria-label={title}
      className={className}
      style={{ fillRule: 'evenodd', clipRule: 'evenodd', strokeLinejoin: 'round', strokeMiterlimit: 2 }}
    >
      <title>{title}</title>
      <path
        d="M590.729,-0l501.146,198.984l0,515.885c0,257.943 -501.146,464.297 -501.146,464.297c0,0 -501.146,-206.354 -501.146,-464.297l0,-515.885l501.146,-198.984Z"
        style={{ fill: '#4f46e5', fillRule: 'nonzero' }}
      />
      <circle cx="590.729" cy="460.755" r="125.286" style={{ fill: '#fff' }} />
      <path
        d="M539.141,523.398l103.177,0l36.849,246.888l-176.875,0l36.849,-246.888Z"
        style={{ fill: '#fff', fillRule: 'nonzero' }}
      />
    </svg>
  )
}

export default KeeStoreLogo
