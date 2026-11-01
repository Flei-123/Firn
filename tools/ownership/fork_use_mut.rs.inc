    pub(crate) fn for_each_use_mut(&mut self, mut rep: impl FnMut(&mut Val)) {
        match self {
            Op::Const(_) | Op::Alloca { .. } | Op::GcAddr { .. } | Op::ThreadSelf => {}
            Op::Bin(_, a, b2) => {
                rep(a);
                rep(b2);
            }
            Op::BinWrapSat { a, b: b2, .. }
            | Op::CheckedBin { a, b: b2, .. }
            | Op::CheckedDiv { a, b: b2, .. } => {
                rep(a);
                rep(b2);
            }
            Op::CheckedCast { src, .. } => rep(src),
            Op::CheckedIdx { idx, .. } => rep(idx),
            Op::Cmp { a, b: b2, .. } => {
                rep(a);
                rep(b2);
            }
            Op::Un(_, a) => rep(a),
            Op::Cast { src, .. } => rep(src),
            Op::Load { addr } => rep(addr),
            Op::Store { addr, val } => {
                rep(addr);
                rep(val);
            }
            Op::PtrAdd { base, off } => {
                rep(base);
                rep(off);
            }
            Op::Simd { args, .. } => {
                for a in args.iter_mut() {
                    rep(a);
                }
            }
            Op::Call { args, .. } | Op::Syscall { args } => {
                for a in args.iter_mut() {
                    rep(a);
                }
            }
            Op::CallIndirect { target, args } => {
                rep(target);
                for a in args.iter_mut() {
                    rep(a);
                }
            }
            Op::VtabAddr { .. } | Op::FnRef { .. } | Op::GlobalAddr { .. } => {}
            Op::CopyMem { dst, src, .. } => {
                rep(dst);
                rep(src);
            }
            Op::AtomicCas { addr, erw, new } => {
                rep(addr);
                rep(erw);
                rep(new);
            }
            Op::ThreadSpawn { arg, stack, ctid } => {
                rep(arg);
                rep(stack);
                rep(ctid);
            }
            Op::AtomicAdd { addr, val } => {
                rep(addr);
                rep(val);
            }
            Op::Phi { incoming } => {
                for (_, v) in incoming.iter_mut() {
                    rep(v);
                }
            }
            Op::Copy { src } => rep(src),
            Op::Select { .. } | Op::Barrier { .. } | Op::SecureZero { .. } => {}
            Op::Asm { .. } | Op::MmioLoad { .. } | Op::MmioStore { .. } => {}
        }
    }

