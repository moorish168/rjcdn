/** Auto-HTTPS / ACME issuance status on a domain. */
export function certStatusLabel(status?: string | null): string {
  switch ((status || 'none').toLowerCase()) {
    case 'active':
      return '已签发'
    case 'pending':
      return '签发中 / 等待 DNS'
    case 'error':
      return '签发失败'
    default:
      return '未签发'
  }
}
