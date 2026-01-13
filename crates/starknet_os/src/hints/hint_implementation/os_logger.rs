use blockifier::state::state_api::StateReader;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintContext;
use crate::hints::vars::Ids;

pub(crate) fn os_logger_enter_syscall_prepare_exit_syscall<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let is_deprecated = true;
    log_enter_syscall_helper(hint_processor, ctx, is_deprecated)
}

pub(crate) fn os_logger_exit_syscall<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    let selector = ctx.get_integer(Ids::Selector)?;
    let range_check_ptr = ctx.get_ptr(Ids::RangeCheckPtr)?;
    Ok(execution_helper.os_logger.exit_syscall(
        selector.try_into()?,
        ctx.vm.get_current_step(),
        range_check_ptr,
        &ctx,
    )?)
}

pub(crate) fn log_enter_syscall<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let is_deprecated = false;
    log_enter_syscall_helper(hint_processor, ctx, is_deprecated)
}

fn log_enter_syscall_helper<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
    is_deprecated: bool,
) -> OsHintResult {
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    let range_check_ptr = ctx.get_ptr(Ids::RangeCheckPtr)?;
    let selector = ctx.get_integer(Ids::Selector)?;

    Ok(execution_helper.os_logger.enter_syscall(
        selector.try_into()?,
        is_deprecated,
        ctx.vm.get_current_step(),
        range_check_ptr,
        &ctx,
    )?)
}
