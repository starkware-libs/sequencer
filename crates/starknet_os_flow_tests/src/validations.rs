use cairo_vm::vm::{errors::runner_errors::RunnerError, runners::cairo_runner::CairoRunner};

#[allow(dead_code)]
pub (crate) fn validate_builtins(runner: &CairoRunner)-> Result<(), RunnerError>{
    let mut stack_ptr = runner.vm.get_ap();
    for builtin_name in runner.get_program().iter_builtins() {
        if let Some(builtin_runner) = runner
            .vm
            .builtin_runners
            .iter_mut()
            .find(|b| b.name() == *builtin_name)
           {
               let new_pointer = builtin_runner.final_stack(&runner.vm.segments, stack_ptr)?;
               stack_ptr = new_pointer;
           }
   }
   let builtins_start = stack_ptr;
   let n_builtins = runner.get_program().builtins_len();
   let builtins_end = runner.vm.get_ap();
   assert_eq!((builtins_start + n_builtins).unwrap(), builtins_end);
   Ok(())
}