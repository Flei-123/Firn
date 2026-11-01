import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;

public class JarDump {
    public static void main(String[] args) throws Exception {
        java.io.PrintStream out = System.out;
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.setOut(out);
        StringBuilder sb = new StringBuilder("{\"blocks\":[");
        boolean first = true;
        for (Block block : BuiltInRegistries.BLOCK) {
            BlockState state = block.defaultBlockState();
            String name = BuiltInRegistries.BLOCK.getKey(block).getPath();
            if (!first) sb.append(',');
            first = false;
            sb.append("{\"name\":\"").append(name).append("\"");
            sb.append(",\"hardness\":").append(state.getDestroySpeed(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
            sb.append(",\"emitLight\":").append(state.getLightEmission());
            sb.append(",\"filterLight\":").append(state.getLightDampening());
            sb.append(",\"fullCollision\":").append(state.isCollisionShapeFullBlock(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
            sb.append(",\"collides\":").append(!state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).isEmpty());
            sb.append(",\"canOcclude\":").append(state.canOcclude());
            sb.append(",\"air\":").append(state.isAir());
            sb.append("}");
        }
        sb.append("],\"entities\":[");
        first = true;
        for (EntityType<?> type : BuiltInRegistries.ENTITY_TYPE) {
            if (!first) sb.append(',');
            first = false;
            sb.append("{\"name\":\"").append(BuiltInRegistries.ENTITY_TYPE.getKey(type).getPath()).append("\"");
            sb.append(",\"width\":").append(type.getWidth());
            sb.append(",\"height\":").append(type.getHeight());
            sb.append(",\"category\":\"").append(type.getCategory().getSerializedName()).append("\"}");
        }
        sb.append("]}");
        Files.writeString(Path.of(args[0]), sb.toString());
    }
}
