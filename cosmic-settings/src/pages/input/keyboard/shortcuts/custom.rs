// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only
//
use std::str::FromStr;

use super::{ShortcutBinding, ShortcutMessage, ShortcutModel};

use cosmic::app::ContextDrawer;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, button, icon};
use cosmic::{Apply, Element, Task};
use cosmic_settings_config::Binding;
use cosmic_settings_config::shortcuts::{Action, Shortcuts};
use cosmic_settings_page::{self as page, Section, section};
use slab::Slab;
use slotmap::{Key, SlotMap};

pub struct Page {
    entity: page::Entity,
    model: super::Model,
    add_shortcut: AddShortcut,
    replace_dialog: Vec<(Binding, Action, String)>,
    task_id: widget::Id,
    name_id: widget::Id,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            entity: page::Entity::null(),
            model: super::Model::default().custom().actions(bindings),
            add_shortcut: AddShortcut::default(),
            replace_dialog: Vec::new(),
            task_id: widget::Id::unique(),
            name_id: widget::Id::unique(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    /// Adds a new key binding input
    AddKeybinding,
    /// Add a new custom shortcut to the config
    AddShortcut,
    /// Update the Task text input
    TaskInput(String),
    /// Toggle editing of the key text input
    EditCombination,
    /// Toggle editability of the key text input
    KeyEditing(usize, bool),
    /// Update the key text input
    KeyInput(usize, String),
    /// Update the name text input
    NameInput(String),
    /// Enter key pressed in the name text input
    NameSubmit,
    /// Apply a requested shortcut replace operation
    ReplaceApply,
    /// Cancel a requested shortcut replace operation
    ReplaceCancel,
    /// Emit a generic shortcut message
    Shortcut(ShortcutMessage),
    /// Open the add shortcut context drawer
    ShortcutContext,
}

#[derive(Default)]
struct AddShortcut {
    pub active: bool,
    pub editing: Option<usize>,
    pub name: String,
    pub task: String,
    pub keys: Slab<(String, widget::Id)>,
}

impl AddShortcut {
    pub fn enable(&mut self) {
        self.active = true;
        self.name.clear();
        self.task.clear();

        if self.keys.is_empty() {
            self.keys.insert((String::new(), widget::Id::unique()));
        } else {
            while self.keys.len() > 1 {
                self.keys.remove(self.keys.len() - 1);
            }

            self.keys[0].0.clear();
        }
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::app::Message> {
        match message {
            Message::TaskInput(text) => {
                self.add_shortcut.task = text;
            }

            Message::KeyInput(id, text) => {
                self.add_shortcut.keys[id].0 = text;
            }

            Message::KeyEditing(id, enable) => {
                if enable {
                    self.add_shortcut.editing = Some(id)
                } else if self.add_shortcut.editing == Some(id) {
                    let task = self.add_keybinding();
                    self.add_shortcut.editing = None;
                    return task;
                }
            }

            Message::NameInput(text) => {
                self.add_shortcut.name = text;
            }

            Message::AddKeybinding => return self.add_keybinding(),

            Message::AddShortcut => {
                let name = self.add_shortcut.name.trim();
                let task = self.add_shortcut.task.trim();

                if name.is_empty() || task.is_empty() {
                    return Task::none();
                }

                let mut addable_bindings = Vec::new();

                for (_, (keys, ..)) in &self.add_shortcut.keys {
                    if keys.is_empty() {
                        continue;
                    }

                    let Ok(binding) = Binding::from_str(keys) else {
                        return Task::none();
                    };

                    if !binding.is_set() {
                        return Task::none();
                    }

                    if let Some(action) = self.model.config_contains(&binding) {
                        let action_str = super::localize_action(&action);
                        self.replace_dialog.push((binding, action, action_str));
                        continue;
                    }

                    addable_bindings.push(binding);
                }

                for binding in addable_bindings {
                    self.add_shortcut(binding);
                }

                self.model.on_enter();
            }

            Message::EditCombination => {
                if let Some((slab_index, (_, id))) = self.add_shortcut.keys.iter().next() {
                    self.add_shortcut.editing = Some(slab_index);
                    return Task::batch(vec![
                        widget::text_input::focus(id.clone()),
                        widget::text_input::select_all(id.clone()),
                    ]);
                }
            }

            Message::NameSubmit => {
                if !self.add_shortcut.name.trim().is_empty() {
                    return widget::text_input::focus(self.task_id.clone());
                }
            }

            Message::ReplaceApply => {
                if let Some((binding, ..)) = self.replace_dialog.pop() {
                    self.model.config_remove(&binding);
                    self.add_shortcut(binding);

                    if self.replace_dialog.is_empty() {
                        self.model.on_enter();
                    }
                }
            }

            Message::ReplaceCancel => {
                _ = self.replace_dialog.pop();
                if self.replace_dialog.is_empty() {
                    self.model.on_enter();
                }
            }

            Message::Shortcut(message) => {
                if let ShortcutMessage::ShowShortcut(..) = message {
                    self.add_shortcut.active = false;
                }

                return self.model.update(message);
            }

            Message::ShortcutContext => {
                self.add_shortcut.enable();
                return Task::batch(vec![
                    cosmic::task::message(crate::app::Message::OpenContextDrawer(self.entity)),
                    widget::text_input::focus(self.name_id.clone()),
                ]);
            }
        }

        Task::none()
    }

    fn add_keybinding(&mut self) -> Task<crate::app::Message> {
        // If an empty entry exists, focus it instead of creating a new input.
        for (_, (binding, id)) in &mut self.add_shortcut.keys {
            if Binding::from_str(binding).is_ok() {
                continue;
            }

            binding.clear();

            return widget::text_input::focus(id.clone());
        }

        let new_id = widget::Id::unique();
        self.add_shortcut.editing = Some(
            self.add_shortcut
                .keys
                .insert((String::new(), new_id.clone())),
        );

        Task::batch(vec![
            widget::text_input::focus(new_id.clone()),
            widget::text_input::select_all(new_id),
        ])
    }

    fn add_keybinding_context(&self) -> Element<'_, Message> {
        let name_input = widget::text_input("", &self.add_shortcut.name)
            .padding([6, 12])
            .on_input(Message::NameInput)
            .on_submit(|_| Message::NameSubmit)
            .id(self.name_id.clone());

        let task_input = widget::text_input("", &self.add_shortcut.task)
            .padding([6, 12])
            .on_input(Message::TaskInput)
            .on_submit(|_| Message::EditCombination)
            .id(self.task_id.clone());

        let name_control = widget::column()
            .spacing(4)
            .push(widget::text::body(fl!("shortcut-name")))
            .push(name_input);

        let command_control = widget::column()
            .spacing(4)
            .push(widget::text::body(fl!("command")))
            .push(task_input);

        let input_fields = widget::column()
            .spacing(12)
            .push(name_control)
            .push(command_control)
            .padding([16, 24]);

        let keys = self.add_shortcut.keys.iter().fold(
            widget::list_column().spacing(0),
            |column, (id, (text, widget_id))| {
                let key_combination = widget::editable_input(
                    fl!("type-key-combination"),
                    text,
                    self.add_shortcut.editing == Some(id),
                    move |enable| Message::KeyEditing(id, enable),
                )
                .select_on_focus(true)
                .padding([0, 12])
                .on_input(move |input| Message::KeyInput(id, input))
                .on_submit(|_| Message::AddKeybinding)
                .id(widget_id.clone())
                .apply(widget::container)
                .padding([8, 24]);

                column.add(key_combination)
            },
        );

        let controls = widget::list_column().add(input_fields).add(keys).spacing(0);

        let add_keybinding_button = widget::button::standard(fl!("add-another-keybinding"))
            .on_press(Message::AddShortcut)
            .apply(widget::container)
            .width(Length::Fill)
            .align_x(Alignment::End);

        widget::column()
            .spacing(32)
            .push(controls)
            .push(add_keybinding_button)
            .into()
    }

    fn add_shortcut(&mut self, mut binding: Binding) {
        self.add_shortcut.active = !self.replace_dialog.is_empty();
        binding.description = Some(self.add_shortcut.name.clone());
        let new_action = Action::Spawn(self.add_shortcut.task.clone());
        self.model.config_add(new_action, binding);
    }
}

impl page::Page<crate::pages::Message> for Page {
    fn set_id(&mut self, entity: page::Entity) {
        self.entity = entity;
        self.model.entity = entity;
    }

    fn info(&self) -> page::Info {
        page::Info::new("custom-shortcuts", "input-keyboard-symbolic")
            .title(fl!("custom-shortcuts"))
    }

    fn content(
        &self,
        sections: &mut SlotMap<section::Entity, Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        Some(vec![sections.insert(shortcuts())])
    }

    fn dialog(&self) -> Option<Element<'_, crate::pages::Message>> {
        // Check if a new shortcut is being added that requires a replace dialog.
        if let Some((binding, _action, action_str)) = self.replace_dialog.last() {
            let primary_action = button::suggested(fl!("replace")).on_press(Message::ReplaceApply);

            let secondary_action = button::standard(fl!("cancel")).on_press(Message::ReplaceCancel);

            let dialog = widget::dialog()
                .title(fl!("replace-shortcut-dialog"))
                .icon(icon::from_name("dialog-warning").size(64))
                .body(fl!(
                    "replace-shortcut-dialog",
                    "desc",
                    shortcut = binding.to_string(),
                    name = action_str.clone()
                ))
                .primary_action(primary_action)
                .secondary_action(secondary_action)
                .apply(Element::from)
                .map(crate::pages::Message::CustomShortcuts);

            return Some(dialog);
        }

        // Check if a keybinding is being added that requires a replace dialog.
        self.model
            .dialog()
            .map(|el| el.map(|m| crate::pages::Message::CustomShortcuts(Message::Shortcut(m))))
    }

    fn context_drawer(&self) -> Option<ContextDrawer<'_, crate::pages::Message>> {
        if self.add_shortcut.active {
            Some(
                cosmic::app::context_drawer(
                    self.add_keybinding_context()
                        .map(crate::pages::Message::CustomShortcuts),
                    crate::pages::Message::CloseContextDrawer,
                )
                .title(fl!("custom-shortcuts", "context")),
            )
        } else {
            self.model.context_drawer(|msg| {
                crate::pages::Message::CustomShortcuts(Message::Shortcut(msg))
            })
        }
    }

    fn on_context_drawer_close(&mut self) -> Task<crate::pages::Message> {
        self.model.on_context_drawer_close();
        Task::none()
    }

    fn on_enter(&mut self) -> Task<crate::pages::Message> {
        self.model.on_enter();
        Task::none()
    }

    fn on_leave(&mut self) -> Task<crate::pages::Message> {
        self.model.on_clear();
        Task::none()
    }
}

impl page::AutoBind<crate::pages::Message> for Page {}

fn bindings(_defaults: &Shortcuts, keybindings: &Shortcuts) -> Slab<ShortcutModel> {
    keybindings
        .iter()
        .fold(Slab::new(), |mut slab, (binding, action)| {
            if let Action::Spawn(task) = action {
                let description = binding
                    .description
                    .clone()
                    .unwrap_or_else(|| task.to_owned());

                let new_binding = ShortcutBinding {
                    id: widget::Id::unique(),
                    binding: binding.clone(),
                    input: String::new(),
                    is_default: false,
                    is_saved: true,
                };

                if let Some((_, existing_model)) =
                    slab.iter_mut().find(|(_, m)| &m.action == action)
                {
                    existing_model.description = description;
                    existing_model.bindings.insert(new_binding);
                } else {
                    slab.insert(ShortcutModel {
                        action: action.clone(),
                        bindings: {
                            let mut slab = Slab::new();
                            slab.insert(new_binding);
                            slab
                        },
                        description,
                        modified: 0,
                    });
                }
            }

            slab
        })
}

fn shortcuts() -> Section<crate::pages::Message> {
    let descriptions = Slab::new();

    // TODO: Add shortcuts to descriptions

    Section::default()
        .descriptions(descriptions)
        .view::<Page>(move |_binder, page, _section| {
            let content = if page.model.shortcut_models.is_empty() {
                widget::settings::section()
                    .add(widget::settings::item_row(vec![
                        widget::text::body(fl!("custom-shortcuts", "none")).into(),
                    ]))
                    .into()
            } else {
                page.model.view().map(Message::Shortcut)
            };

            let add_shortcut = widget::button::standard(fl!("custom-shortcuts", "add"))
                .on_press(Message::ShortcutContext)
                .apply(widget::container)
                .width(Length::Fill)
                .align_x(Alignment::End);

            widget::column()
                .push(content)
                .push(add_shortcut)
                .spacing(24)
                .apply(Element::from)
                .map(crate::pages::Message::CustomShortcuts)
        })
}
