use cairo_vm::vm::runners::cairo_runner::CairoRunner;

#[allow(dead_code)]
pub (crate) fn validate_builtins(runner: &mut CairoRunner){
   let builtins_start = runner.get_builtins_final_stack(runner.vm.get_ap()).unwrap();
   let n_builtins = runner.get_program().builtins_len();
   let builtins_end = runner.vm.get_ap();
   assert_eq!((builtins_start + n_builtins).unwrap(), builtins_end);
}