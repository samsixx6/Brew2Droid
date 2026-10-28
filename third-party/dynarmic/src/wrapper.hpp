#include <dynarmic/interface/A32/a32.h>
#include <dynarmic/interface/A32/coprocessor.h>
#include <dynarmic/interface/A64/a64.h>
#include <dynarmic/interface/exclusive_monitor.h>

extern "C" {
static_assert(sizeof(std::optional<std::uint32_t>) == 8, "Failed to verify size of type std::optional<u32>");
static_assert(sizeof(std::optional<std::uint64_t>) == 16, "Failed to verify size of type std::optional<u64>");
static_assert(sizeof(std::optional<std::uintptr_t>) == 16, "Failed to verify size of type std::optional<usize>");
static_assert(sizeof(Dynarmic::A32::UserConfig) == 368, "Failed to verify size of type A32::UserConfig");
static_assert(sizeof(Dynarmic::A64::UserConfig) == 144, "Failed to verify size of type A64::UserConfig");
static_assert(sizeof(std::shared_ptr<Dynarmic::A32::Coprocessor>) == 16,
              "Failed to verify size of type std::shared_ptr");
static_assert(sizeof(Dynarmic::ExclusiveMonitor) == 56, "Failed to verify size of type ExclusiveMonitor");
static_assert(sizeof(Dynarmic::SpinLock) == sizeof(std::int32_t), "Failed to verify size of type SpinLock");

Dynarmic::HaltReason JitA32_Run(Dynarmic::A32::Jit *jit) {
    return jit->Run();
}

Dynarmic::HaltReason JitA32_Step(Dynarmic::A32::Jit *jit) {
    return jit->Step();
}

void JitA32_ClearCache(Dynarmic::A32::Jit *jit) {
    jit->ClearCache();
}

void JitA32_InvalidateCacheRange(Dynarmic::A32::Jit *jit, std::uint32_t start, size_t length) {
    jit->InvalidateCacheRange(start, length);
}

void JitA32_Reset(Dynarmic::A32::Jit *jit) {
    jit->Reset();
}

void JitA32_HaltExecution(Dynarmic::A32::Jit *jit, Dynarmic::HaltReason hr) {
    jit->HaltExecution(hr);
}

void JitA32_ClearHalt(Dynarmic::A32::Jit *jit, Dynarmic::HaltReason hr) {
    jit->ClearHalt(hr);
}

std::array<std::uint32_t, 16> *JitA32_Regs(Dynarmic::A32::Jit *jit) {
    return &jit->Regs();
}

std::array<std::uint32_t, 64> *JitA32_ExtRegs(Dynarmic::A32::Jit *jit) {
    return &jit->ExtRegs();
}

std::uint32_t JitA32_Cpsr(Dynarmic::A32::Jit *jit) {
    return jit->Cpsr();
}

void JitA32_SetCpsr(Dynarmic::A32::Jit *jit, std::uint32_t val) {
    jit->SetCpsr(val);
}

std::uint32_t JitA32_Fpscr(Dynarmic::A32::Jit *jit) {
    return jit->Fpscr();
}

void JitA32_SetFpscr(Dynarmic::A32::Jit *jit, std::uint32_t val) {
    jit->SetFpscr(val);
}

void JitA32_ClearExclusiveState(Dynarmic::A32::Jit *jit) {
    jit->ClearExclusiveState();
}


Dynarmic::HaltReason JitA64_Run(Dynarmic::A64::Jit *jit) {
    return jit->Run();
}

Dynarmic::HaltReason JitA64_Step(Dynarmic::A64::Jit *jit) {
    return jit->Step();
}

void JitA64_ClearCache(Dynarmic::A64::Jit *jit) {
    jit->ClearCache();
}

void JitA64_InvalidateCacheRange(Dynarmic::A64::Jit *jit, std::uint64_t start, size_t length) {
    jit->InvalidateCacheRange(start, length);
}

void JitA64_Reset(Dynarmic::A64::Jit *jit) {
    jit->Reset();
}

void JitA64_haltexecution(Dynarmic::A64::Jit *jit, Dynarmic::HaltReason hr) {
    jit->HaltExecution(hr);
}

void JitA64_ClearHalt(Dynarmic::A64::Jit *jit, Dynarmic::HaltReason hr) {
    jit->ClearHalt(hr);
}

std::uint64_t JitA64_GetSP(Dynarmic::A64::Jit *jit) {
    return jit->GetSP();
}

void JitA64_SetSP(Dynarmic::A64::Jit *jit, std::uint64_t val) {
    return jit->SetSP(val);
}

std::uint64_t JitA64_GetPC(Dynarmic::A64::Jit *jit) {
    return jit->GetPC();
}

void JitA64_SetPC(Dynarmic::A64::Jit *jit, std::uint64_t val) {
    return jit->SetPC(val);
}

std::uint64_t JitA64_GetReg(Dynarmic::A64::Jit *jit, size_t index) {
    return jit->GetRegister(index);
}

void JitA64_SetReg(Dynarmic::A64::Jit *jit, size_t index, std::uint64_t val) {
    return jit->SetRegister(index, val);
}

void JitA64_GetRegs(Dynarmic::A64::Jit *jit, std::array<std::uint64_t, 31> *out) {
    *out = jit->GetRegisters();
}

void JitA64_SetRegs(Dynarmic::A64::Jit *jit, std::array<std::uint64_t, 31> *regs) {
    return jit->SetRegisters(*regs);
}

void JitA64_GetVector(Dynarmic::A64::Jit *jit, std::array<std::uint64_t, 2> *out, size_t index) {
    *out = jit->GetVector(index);
}

void JitA64_SetVector(Dynarmic::A64::Jit *jit, size_t index, std::array<std::uint64_t, 2> *val) {
    return jit->SetVector(index, *val);
}

void JitA64_GetVectors(Dynarmic::A64::Jit *jit, std::array<std::array<std::uint64_t, 2>, 32> *out) {
    auto vec = jit->GetVectors();
    memcpy(out, &vec, sizeof(vec));
}

void JitA64_SetVectors(Dynarmic::A64::Jit *jit, std::array<std::array<std::uint64_t, 2>, 32> *regs) {
    return jit->SetVectors(*regs);
}

std::uint32_t JitA64_GetFpcr(Dynarmic::A64::Jit *jit) {
    return jit->GetFpcr();
}

void JitA64_SetFpcr(Dynarmic::A64::Jit *jit, std::uint32_t val) {
    return jit->SetFpcr(val);
}

std::uint32_t JitA64_GetFpsr(Dynarmic::A64::Jit *jit) {
    return jit->GetFpsr();
}

void JitA64_SetFpsr(Dynarmic::A64::Jit *jit, std::uint32_t val) {
    return jit->SetFpsr(val);
}

std::uint32_t JitA64_GetPstate(Dynarmic::A64::Jit *jit) {
    return jit->GetPstate();
}

void JitA64_SetPstate(Dynarmic::A64::Jit *jit, std::uint32_t val) {
    return jit->SetPstate(val);
}

void JitA64_ClearExclusiveState(Dynarmic::A64::Jit *jit) {
    jit->ClearExclusiveState();
}

bool JitA64_IsExecuting(Dynarmic::A64::Jit *jit) {
    return jit->IsExecuting();
}

void ExclusiveMonitor_ExclusiveMonitor(Dynarmic::ExclusiveMonitor *self, size_t processor_count) {
    new(self) Dynarmic::ExclusiveMonitor(processor_count);
}

void ExclusiveMonitor_ClearProcessor(Dynarmic::ExclusiveMonitor *self, size_t processor_id) {
    self->ClearProcessor(processor_id);
}

void ExclusiveMonitor_Clear(Dynarmic::ExclusiveMonitor *self) {
    self->Clear();
}

size_t ExclusiveMonitor_GetProcessorCount(Dynarmic::ExclusiveMonitor *self) {
    return self->GetProcessorCount();
}

void SpinLock_Lock(Dynarmic::SpinLock *lock) {
    lock->Lock();
}

void SpinLock_Unlock(Dynarmic::SpinLock *lock) {
    lock->Unlock();
}

std::uint64_t *get_vec_u64(std::vector<std::uint64_t> *vec, size_t index) {
    return &(*vec)[index];
}

size_t size_vec_u64(std::vector<std::uint64_t> *vec) {
    return vec->size();
}

std::uint64_t *get_vec_u128(std::vector<std::array<std::uint64_t, 2> > *vec, size_t index) {
    return (*vec)[index].data();
}

void delete_vec_u64(std::vector<std::uint64_t> *vec) {
    vec->~vector();
}

void delete_vec_u128(std::vector<Dynarmic::Vector> *vec) {
    vec->~vector();
}

void new_optional_usize(std::optional<std::uintptr_t> *out, std::uintptr_t s) {
    if (s == 0) {
        *out = std::nullopt;
        return;
    }
    *out = std::optional(s);
    return;
}

void new_optional_u32(std::optional<std::uint32_t> *out, std::uint32_t s) {
    if (s == 0) {
        *out = std::nullopt;
        return;
    }
    *out = std::optional(s);
    return;
}

void new_coprocessor(std::shared_ptr<Dynarmic::A32::Coprocessor> *out, Dynarmic::A32::Coprocessor *ptr) {
    if (ptr == nullptr) {
        *out = std::shared_ptr<Dynarmic::A32::Coprocessor>();
        return;
    }
    void *copied = malloc(sizeof(Dynarmic::A32::Coprocessor));
    memcpy(copied, ptr, sizeof(Dynarmic::A32::Coprocessor));
    *out = std::shared_ptr<Dynarmic::A32::Coprocessor>((Dynarmic::A32::Coprocessor *) copied,
                                                       [](Dynarmic::A32::Coprocessor *p) { free(p); });
}

Dynarmic::A32::Jit *new_a32_jit(Dynarmic::A32::UserConfig *conf) {
    return new Dynarmic::A32::Jit(*conf);
}

void delete_a32_jit(Dynarmic::A32::Jit *ptr) {
    delete ptr;
}

Dynarmic::A64::Jit *new_a64_jit(Dynarmic::A64::UserConfig *conf) {
    return new Dynarmic::A64::Jit(*conf);
}

void delete_a64_jit(Dynarmic::A64::Jit *ptr) {
    delete ptr;
}
} // extern