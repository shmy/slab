use module::DomainModule;

/// 全部模块。health 仅提供公开端点，受保护遍历无副作用。
pub(crate) const MODULES: &[&dyn DomainModule] = &[
    &audit::Module,
    &identity::Module,
    &file::Module,
    &health::Module,
    &item::Module,
    &customer::Module,
    &supplier::Module,
    &finance::Module,
    &warehouse::Module,
    &purchase::Module,
    &quality::Module,
    &sales::Module,
    &product::Module,
    &production::Module,
    &planning::Module,
];
