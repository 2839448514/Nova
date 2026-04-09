<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { Button } from '@/components/ui/button'

const props = withDefaults(defineProps<{
  modelValue: boolean
  title: string
  description?: string
  confirmText?: string
  cancelText?: string
  busy?: boolean
  destructive?: boolean
}>(), {
  description: '',
  confirmText: 'Confirm',
  cancelText: 'Cancel',
  busy: false,
  destructive: false,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'confirm'): void
}>()

const confirmVariant = computed(() => (props.destructive ? 'destructive' : 'default'))

const close = () => {
  if (props.busy) return
  emit('update:modelValue', false)
}

const handleConfirm = () => {
  if (props.busy) return
  emit('confirm')
}

const handleKeydown = (event: KeyboardEvent) => {
  if (!props.modelValue) return
  if (event.key === 'Escape') {
    event.preventDefault()
    close()
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
})
</script>

<template>
  <Teleport to="body">
    <Transition name="confirm-backdrop">
      <div
        v-if="modelValue"
        class="fixed inset-0 z-[95] flex items-center justify-center bg-[rgba(24,18,10,0.36)] px-5 backdrop-blur-[4px]"
        @click.self="close"
      >
        <Transition name="confirm-card">
          <div
            v-if="modelValue"
            class="w-full max-w-[460px] rounded-[24px] border border-[#e8dfd1] bg-[linear-gradient(180deg,#fffdf9_0%,#fbf7f1_100%)] p-6 shadow-[0_24px_70px_rgba(40,28,16,0.18)] dark:border-[#3d3932] dark:bg-[linear-gradient(180deg,#2d2a26_0%,#24221f_100%)]"
          >
            <div class="flex items-start justify-between gap-4">
              <div>
                <div class="text-[19px] font-semibold tracking-[-0.02em] text-[#211d17] dark:text-[#f3eee7]">
                  {{ title }}
                </div>
                <div
                  v-if="description"
                  class="mt-3 text-[14px] leading-6 text-[#756d62] dark:text-[#b5aea4]"
                >
                  {{ description }}
                </div>
              </div>

              <button
                type="button"
                class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-[#8f8577] transition-colors hover:bg-[#efe7da] hover:text-[#352d23] dark:text-[#9e9588] dark:hover:bg-[#3a362f] dark:hover:text-[#f3eee7]"
                :disabled="busy"
                @click="close"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="h-4 w-4">
                  <path d="M6 6l12 12M18 6L6 18" stroke-linecap="round" />
                </svg>
              </button>
            </div>

            <div class="mt-6 flex items-center justify-end gap-3">
              <Button
                variant="outline"
                size="sm"
                class="border-[#ddd3c5] bg-white/70 text-[#5b5449] hover:bg-[#f5efe5] dark:border-[#4a453e] dark:bg-[#2d2b27] dark:text-[#d2cbc2] dark:hover:bg-[#35322d]"
                :disabled="busy"
                @click="close"
              >
                {{ cancelText }}
              </Button>
              <Button
                :variant="confirmVariant"
                size="sm"
                class="min-w-[96px]"
                :disabled="busy"
                @click="handleConfirm"
              >
                {{ confirmText }}
              </Button>
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.confirm-backdrop-enter-active,
.confirm-backdrop-leave-active {
  transition: opacity 0.2s ease;
}

.confirm-backdrop-enter-from,
.confirm-backdrop-leave-to {
  opacity: 0;
}

.confirm-card-enter-active {
  transition: opacity 0.22s ease, transform 0.22s ease;
}

.confirm-card-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}

.confirm-card-enter-from,
.confirm-card-leave-to {
  opacity: 0;
  transform: translateY(10px) scale(0.98);
}
</style>
