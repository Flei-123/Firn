import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

public class EquipDump {
    public static void main(String[] args) {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        for (Item item : BuiltInRegistries.ITEM) {
            ItemStack stack = new ItemStack(item);
            EquipmentSlot slot = Mob.getEquipmentSlotForItem(stack);
            String name = BuiltInRegistries.ITEM.getKey(item).getPath();
            System.out.println(name + " " + slot.getName() + " " + item.getMaxStackSize());
        }
    }
}
