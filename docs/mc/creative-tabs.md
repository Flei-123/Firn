# Creative inventory tab grouping

Vanilla does not ship the creative tab contents as data (code only). mcplay derives them from item names:

- Rules live in `lib/mc/mc_creative_rules.fi` (exact / prefix / suffix lists per tab, OR-ed into a bit mask per item; one item can be in several tabs, e.g. axes in Tools and Combat, doors in Building and Redstone).
- Colored Blocks: colour prefix + wool/carpet/terracotta/glazed_terracotta/concrete/concrete_powder/stained_glass(_pane)/bed/candle/banner/shulker_box.
- Items without any rule: block items go to Building Blocks, non-block items to Ingredients.
- Hidden items (op-only / technical) are listed in `HIDDEN_EXACT` / `HIDDEN_PREFIX`.
- Order inside a tab = registry id order (close to vanilla for blocks, not identical).
- Search tab = every non-hidden item whose name (underscores as spaces) contains the query.
- Not modelled: Operator Utilities, Saved Hotbars, NBT variants (potions, enchanted books, tipped arrows, firework stars).
