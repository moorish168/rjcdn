/**
 * Focus trap + Esc/Enter helpers for modal / drawer / confirm overlays.
 */
export function useOverlayFocus(options: {
  open: Ref<boolean> | ComputedRef<boolean>
  root: Ref<HTMLElement | null>
  onEsc?: () => void
  /** Enter confirms unless focus is in textarea/select/contenteditable. */
  onEnter?: () => void
  /** Prefer this selector after open (e.g. primary action). */
  initialFocus?: string
}) {
  let previous: HTMLElement | null = null

  function focusables(root: HTMLElement): HTMLElement[] {
    return Array.from(
      root.querySelectorAll<HTMLElement>(
        'a[href],button:not([disabled]),textarea:not([disabled]),input:not([disabled]):not([type="hidden"]),select:not([disabled]),[tabindex]:not([tabindex="-1"])',
      ),
    ).filter((el) => !el.hasAttribute('disabled') && el.offsetParent !== null)
  }

  function focusInitial() {
    const root = options.root.value
    if (!root) return
    nextTick(() => {
      if (options.initialFocus) {
        const prefer = root.querySelector<HTMLElement>(options.initialFocus)
        if (prefer) {
          prefer.focus()
          return
        }
      }
      const list = focusables(root)
      const firstInput = list.find(
        (el) =>
          el.tagName === 'INPUT'
          || el.tagName === 'TEXTAREA'
          || el.tagName === 'SELECT'
          || el.isContentEditable,
      )
      ;(firstInput || list[0] || root).focus()
    })
  }

  function onKeydown(e: KeyboardEvent) {
    if (!options.open.value) return
    const root = options.root.value
    if (!root) return

    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      options.onEsc?.()
      return
    }

    if (e.key === 'Enter' && options.onEnter) {
      const t = e.target as HTMLElement | null
      if (
        t
        && (t.tagName === 'TEXTAREA'
          || t.tagName === 'SELECT'
          || t.isContentEditable
          || t.closest('[data-no-enter-confirm]'))
      ) {
        return
      }
      // Let native button/form submit handle itself when already on a button.
      if (t?.tagName === 'BUTTON' || t?.closest('button')) return
      e.preventDefault()
      options.onEnter()
      return
    }

    if (e.key !== 'Tab') return
    const list = focusables(root)
    if (!list.length) {
      e.preventDefault()
      return
    }
    const first = list[0]
    const last = list[list.length - 1]
    const active = document.activeElement as HTMLElement | null
    if (e.shiftKey) {
      if (active === first || !root.contains(active)) {
        e.preventDefault()
        last.focus()
      }
    } else if (active === last || !root.contains(active)) {
      e.preventDefault()
      first.focus()
    }
  }

  watch(
    () => options.open.value,
    (open) => {
      if (open) {
        previous = document.activeElement as HTMLElement | null
        document.addEventListener('keydown', onKeydown, true)
        focusInitial()
      } else {
        document.removeEventListener('keydown', onKeydown, true)
        if (previous && typeof previous.focus === 'function') {
          previous.focus()
        }
        previous = null
      }
    },
  )

  onBeforeUnmount(() => {
    document.removeEventListener('keydown', onKeydown, true)
  })
}
