# pf_ctx_watch.py - engine gdb-Python do harness watch_pf_writer.ps1 (s488).
#
# PROBLEMA QUE RESOLVE
#   O watchpoint por endereco fixo so cobre o PRIMEIRO contexto publicado. O Box
#   do TernaryJobCtx e reciclado em varios enderecos (0x4000008c6ee0,
#   0x400000e19660, ...), entao o storm pode cair num endereco nao vigiado.
#
# O QUE FAZ
#   - breakpoint no publish (`AtomicPtr<TernaryJobCtx>::store`, rsi = ctx);
#   - a cada publish com rsi != 0: apaga o watchpoint anterior e arma um novo em
#     ctx+0x10 (campo c_ptr), classe WRITE;
#   - o watchpoint so RETEM o alvo quando o valor novo e' ZERO (o write que arma
#     o storm). Metadados do allocator (ex.: talc::register_gap gravando 0x38) e
#     o Box::new reciclando o endereco seguem sem parar.
#   - ao parar: seta $ctx e imprime o PC do writer. O script gdb faz o dump.
#
# ASCII-only.

import gdb

PUBLISH_ADDR = 0
CTX_OFF = 0x10

# Mantem os ultimos MAX_WATCH ctxs vigiados: o storm nasce de um worker ATRASADO
# de um dispatch ANTERIOR, cujo bloco e' liberado/reaproveitado depois de o BSP
# ja ter publicado um ctx novo. Vigiando so o ctx atual, a escrita no bloco
# antigo passa batida.
MAX_WATCH = 3

_state = {"watch": [], "ctx": 0, "publishes": 0, "watch_hits": 0}


def _u64(expr):
    return int(gdb.parse_and_eval("*(unsigned long*)(%s)" % expr))


class PublishBP(gdb.Breakpoint):
    def __init__(self):
        gdb.Breakpoint.__init__(self, "*0x%x" % PUBLISH_ADDR,
                                gdb.BP_BREAKPOINT, internal=False)

    def stop(self):
        try:
            rsi = int(gdb.parse_and_eval("$rsi"))
        except gdb.error:
            return False
        if rsi == 0:
            return False  # T_CTX.store(null) = fim de dispatch
        ctx = rsi
        try:
            cptr = _u64("0x%x" % (ctx + CTX_OFF))
        except gdb.error:
            cptr = 0
        _state["ctx"] = ctx
        _state["publishes"] += 1
        gdb.execute("set $ctx = 0x%x" % ctx, to_string=True)
        if _state["publishes"] <= 40:
            print("[WP] PUBLISH ctx=0x%x c_ptr=0x%x (n=%d)"
                  % (ctx, cptr, _state["publishes"]))
        ws = _state["watch"]
        if len(ws) >= MAX_WATCH:
            old = ws.pop(0)
            try:
                old.delete()
            except Exception:
                pass
        try:
            ws.append(ZeroWatch(ctx + CTX_OFF))
        except Exception as e:
            print("[WP] watch fail @0x%x: %s" % (ctx + CTX_OFF, e))
        return False  # nunca para no publish


class ZeroWatch(gdb.Breakpoint):
    def __init__(self, addr):
        gdb.Breakpoint.__init__(self, "*(unsigned long*)0x%x" % addr,
                                gdb.BP_WATCHPOINT, internal=False,
                                wp_class=gdb.WP_WRITE)
        self.addr = addr
        self.silent = True

    def stop(self):
        try:
            v = _u64("0x%x" % self.addr)
        except gdb.error:
            return False
        _state["watch_hits"] += 1
        if v != 0:
            # Bloco reciclado pelo allocator (free-list/gap) ou Box::new.
            if _state["watch_hits"] <= 12:
                print("[WP] write nao-zero addr=0x%x val=0x%x rip=0x%x (segue)"
                      % (self.addr, v, int(gdb.parse_and_eval("$rip"))))
            return False
        rip = int(gdb.parse_and_eval("$rip"))
        print("[WP] ZERO-WRITER rip=0x%x addr=0x%x ctx=0x%x"
              % (rip, self.addr, _state["ctx"]))
        return True  # retem o alvo: o script gdb faz o dump


def init(publish_addr, ctx_off=0x10):
    global PUBLISH_ADDR, CTX_OFF
    PUBLISH_ADDR = int(publish_addr)
    CTX_OFF = int(ctx_off)
    PublishBP()
    print("[WP] engine pronta: publish=0x%x ctx_off=0x%x"
          % (PUBLISH_ADDR, CTX_OFF))
