use cairo_vm::vm::runners::cairo_runner::CairoRunner;

#[allow(dead_code)]
pub (crate) fn validate_builtins(runner: &mut CairoRunner){
let mut stack_ptr = runner.vm.get_ap();
let builtin_names: Vec<_> = runner.get_program().iter_builtins().cloned().collect();
for builtin_name in builtin_names {
    if let Some(builtin_runner) = runner
        .vm
        .builtin_runners
        .iter_mut()
        .find(|b| b.name() == builtin_name)
    {
        stack_ptr = builtin_runner.final_stack(&runner.vm.segments, stack_ptr).unwrap_or_else(|err| panic!("{}",err));
    }
}
   let builtins_start = stack_ptr;
   let n_builtins = runner.get_program().builtins_len();
   let builtins_end = runner.vm.get_ap();
   assert_eq!((builtins_start + n_builtins).unwrap(), builtins_end);
}