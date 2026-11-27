import java.awt.*;
import java.awt.geom.*;
import java.awt.image.BufferedImage;
import java.awt.image.DataBufferByte;
import java.io.*;
import java.nio.file.*;
import java.util.Locale;
import java.util.Random;
import javax.imageio.ImageIO;

/**
 * Generates deterministic 100% synthetic 1920x1080/60 flight footage (7.0 Obsidian Cinema) for offline demo streams.
 * Diegetic cockpit HUD stays strictly inside the center-safe band (x: 620..1300, y: 310..620) so Nightjar's top-left
 * performance stats, right Control Center dock, bottom touch controls, and PC keyboard never collide with video text.
 */
public class GenerateAssets {
    static final Color AMBER = new Color(245, 176, 65);
    static final Color AMBER_HI = new Color(255, 200, 102);
    static final Color CYAN = new Color(56, 189, 248);
    static final Color INK = new Color(245, 247, 250);
    static final Color MUTED = new Color(199, 205, 217);

    static Font fontRegular;
    static Font fontSemibold;

    static Font loadBundledFont(Path assetsDir, String fileName, int fallbackStyle) {
        Path[] roots = new Path[] {
            assetsDir.toAbsolutePath().normalize(),
            Path.of("").toAbsolutePath().normalize()
        };
        for (Path start : roots) {
            for (Path cur = start; cur != null; cur = cur.getParent()) {
                for (String rel : new String[] {
                    "protocol/desktop/assets/" + fileName,
                    "client/protocol/desktop/assets/" + fileName
                }) {
                    Path candidate = cur.resolve(rel);
                    if (Files.isRegularFile(candidate)) {
                        try (InputStream in = Files.newInputStream(candidate)) {
                            return Font.createFont(Font.TRUETYPE_FONT, in);
                        } catch (Exception ignored) {
                            // Fall through to next candidate or logical fallback.
                        }
                    }
                }
            }
        }
        return new Font(Font.SANS_SERIF, fallbackStyle, 1);
    }

    static void initFonts(Path assetsDir) {
        fontRegular = loadBundledFont(assetsDir, "Figtree-400.ttf", Font.PLAIN);
        fontSemibold = loadBundledFont(assetsDir, "Figtree-600.ttf", Font.BOLD);
    }

    static Graphics2D graphics(BufferedImage image) {
        Graphics2D g = image.createGraphics();
        g.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);
        g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
        g.setRenderingHint(RenderingHints.KEY_FRACTIONALMETRICS, RenderingHints.VALUE_FRACTIONALMETRICS_ON);
        g.setRenderingHint(RenderingHints.KEY_STROKE_CONTROL, RenderingHints.VALUE_STROKE_PURE);
        return g;
    }

    static void text(Graphics2D g, Font base, String str, float size, float x, float y, Color color) {
        g.setFont(base.deriveFont(size));
        g.setColor(color);
        g.drawString(str, x, y);
    }

    static void textCentered(Graphics2D g, Font base, String str, float size, float cx, float y, Color color) {
        Font derived = base.deriveFont(size);
        g.setFont(derived);
        g.setColor(color);
        FontMetrics fm = g.getFontMetrics(derived);
        g.drawString(str, cx - fm.stringWidth(str) / 2f, y);
    }

    static void glow(Graphics2D g, float x, float y, float radius, Color color) {
        g.setPaint(new RadialGradientPaint(x, y, radius, new float[] {0f, 1f},
                new Color[] {color, new Color(color.getRed(), color.getGreen(), color.getBlue(), 0)}));
        g.fill(new Ellipse2D.Float(x - radius, y - radius, radius * 2, radius * 2));
    }

    static BufferedImage background() {
        BufferedImage image = new BufferedImage(1920, 1080, BufferedImage.TYPE_3BYTE_BGR);
        Graphics2D g = graphics(image);
        g.setPaint(new GradientPaint(0, 0, new Color(9, 10, 13), 0, 820, new Color(19, 24, 38)));
        g.fillRect(0, 0, 1920, 1080);
        glow(g, 1040, 380, 520, new Color(56, 189, 248, 52));
        glow(g, 960, 540, 620, new Color(245, 176, 65, 58));
        Random random = new Random(54);
        for (int i = 0; i < 320; i++) {
            g.setColor(new Color(214, 226, 248, random.nextInt(135) + 45));
            int size = random.nextInt(2) + 1;
            g.fillOval(random.nextInt(1920), random.nextInt(640), size, size);
        }
        // Crescent ringed planet positioned in upper-mid sky away from top-left stats and right dock.
        int px = 1095, py = 235, pr = 115;
        g.setPaint(new GradientPaint(px - pr, py - pr, new Color(245, 176, 65), px + pr, py + pr, new Color(20, 26, 40)));
        g.fillOval(px - pr, py - pr, pr * 2, pr * 2);
        g.setColor(new Color(12, 16, 26, 225));
        g.fillOval(px - pr + 26, py - pr + 14, pr * 2 - 18, pr * 2 - 18);
        AffineTransform saved = g.getTransform();
        g.rotate(-0.21, px, py);
        g.setColor(new Color(245, 176, 65, 95));
        g.setStroke(new BasicStroke(3f));
        g.drawOval(px - 205, py - 34, 410, 68);
        g.setColor(new Color(56, 189, 248, 65));
        g.setStroke(new BasicStroke(1.5f));
        g.drawOval(px - 232, py - 40, 464, 80);
        g.setTransform(saved);
        g.dispose();
        return image;
    }

    static void drawDiegeticHud(Graphics2D g, double phase) {
        // Strictly bounded inside x: 650..1270, y: 325..585 (clear of stats, Control Center, touch pads, keyboard).
        int aimX = 960 + (int) Math.round(Math.sin(phase + 0.5) * 72);
        int aimY = 468 + (int) Math.round(Math.cos(phase) * 18);

        // Subtle DEMO telemetry header pill at top of center-safe band (x: 866..1054, y: 328..354).
        g.setColor(new Color(9, 10, 13, 155));
        g.fill(new RoundRectangle2D.Float(866, 328, 188, 26, 13, 13));
        g.setColor(new Color(255, 255, 255, 34));
        g.setStroke(new BasicStroke(1f));
        g.draw(new RoundRectangle2D.Float(866, 328, 188, 26, 13, 13));
        textCentered(g, fontSemibold, "DEMO  \u00B7  SECTOR 07", 11.5f, 960f, 345f, MUTED);

        // Outer translucent cockpit bracket arcs around reticle.
        g.setColor(new Color(56, 189, 248, 135));
        g.setStroke(new BasicStroke(1.8f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
        g.draw(new Arc2D.Float(aimX - 88, aimY - 88, 176, 176, 142, 76, Arc2D.OPEN));
        g.draw(new Arc2D.Float(aimX - 88, aimY - 88, 176, 176, -38, 76, Arc2D.OPEN));

        // Inner precision reticle & warm amber flight-path pip.
        g.setColor(new Color(56, 189, 248, 175));
        g.setStroke(new BasicStroke(1.5f));
        g.draw(new Ellipse2D.Float(aimX - 24, aimY - 24, 48, 48));
        g.drawLine(aimX - 40, aimY, aimX - 16, aimY);
        g.drawLine(aimX + 16, aimY, aimX + 40, aimY);
        g.drawLine(aimX, aimY - 38, aimX, aimY - 16);
        g.setColor(AMBER);
        g.fill(new Ellipse2D.Float(aimX - 3.5f, aimY - 3.5f, 7f, 7f));

        // Pitch-ladder micro ticks inside arc.
        g.setColor(new Color(199, 205, 217, 90));
        for (int dy : new int[] {-36, 36}) {
            g.drawLine(aimX - 56, aimY + dy, aimX - 38, aimY + dy);
            g.drawLine(aimX + 38, aimY + dy, aimX + 56, aimY + dy);
        }

        // Left compact telemetry readout (x >= 680, y ~ 442..494).
        float lx = aimX - 202f, ly = aimY - 25f;
        g.setColor(new Color(9, 10, 13, 162));
        g.fill(new RoundRectangle2D.Float(lx, ly, 98, 50, 12, 12));
        g.setColor(new Color(56, 189, 248, 80));
        g.draw(new RoundRectangle2D.Float(lx, ly, 98, 50, 12, 12));
        text(g, fontSemibold, "SPD", 11f, lx + 12f, ly + 18f, CYAN);
        int speed = 284 + (int) Math.round(16 * Math.sin(phase));
        text(g, fontSemibold, String.format(Locale.ROOT, "%03d KT", speed), 18f, lx + 12f, ly + 40f, INK);

        // Right compact telemetry & shield micro-bar (x <= 1236, y ~ 442..494).
        float rx = aimX + 104f, ry = aimY - 25f;
        g.setColor(new Color(9, 10, 13, 162));
        g.fill(new RoundRectangle2D.Float(rx, ry, 104, 50, 12, 12));
        g.setColor(new Color(245, 176, 65, 85));
        g.draw(new RoundRectangle2D.Float(rx, ry, 104, 50, 12, 12));
        text(g, fontSemibold, "THRUST", 11f, rx + 12f, ry + 18f, AMBER);
        g.setColor(new Color(38, 44, 58));
        g.fill(new RoundRectangle2D.Float(rx + 12f, ry + 29f, 80f, 8f, 4f, 4f));
        float fillW = 64f + (float) (10.0 * Math.sin(phase * 2));
        g.setColor(AMBER);
        g.fill(new RoundRectangle2D.Float(rx + 12f, ry + 29f, fillW, 8f, 4f, 4f));
    }

    static void frame(Graphics2D g, BufferedImage background, int frame) {
        double t = frame / 60.0, phase = t * Math.PI / 5.0;
        g.drawImage(background, 0, 0, null);

        // Parallax obsidian mountain ridges with warm amber & cyan rim highlights.
        for (int layer = 0; layer < 4; layer++) {
            Path2D ridge = new Path2D.Double();
            ridge.moveTo(0, 1080);
            for (int x = 0; x <= 1940; x += 20) {
                double world = x / 230.0 + phase * (layer + 1);
                double y = 535 + layer * 64 - Math.abs(Math.sin(world) * 82 + Math.sin(world * 3) * 30);
                ridge.lineTo(x, y);
            }
            ridge.lineTo(1940, 1080);
            ridge.closePath();
            g.setColor(new Color(28 - layer * 5, 33 - layer * 6, 48 - layer * 8));
            g.fill(ridge);
            g.setColor(layer < 2 ? new Color(245, 176, 65, 78 - layer * 22) : new Color(56, 189, 248, 66 - layer * 12));
            g.setStroke(new BasicStroke(1.8f));
            g.draw(ridge);
        }

        // Perspective Obsidian Cinema canyon flight deck.
        g.setPaint(new GradientPaint(0, 695, new Color(13, 17, 26), 0, 1080, new Color(5, 6, 8)));
        g.fillRect(0, 695, 1920, 385);
        g.setStroke(new BasicStroke(1.6f));
        for (int x = -15; x <= 15; x++) {
            g.setColor(new Color(56, 189, 248, 44));
            g.drawLine(960 + x * 30, 695, 960 + x * 220, 1080);
        }
        for (int i = 0; i < 22; i++) {
            double z = (i / 22.0 + t / 5.0) % 1.0;
            int y = 695 + (int) (z * z * 385);
            g.setColor(new Color(56, 189, 248, (int) (28 + z * 58)));
            g.drawLine(0, y, 1920, y);
        }

        // Passing architectural canyon pylons accented with amber and cyan beacons.
        for (int i = 0; i < 26; i++) {
            double z = (i / 26.0 + t / 10.0) % 1.0;
            double scale = 0.15 + z * z * 1.75;
            int side = i % 2 == 0 ? -1 : 1;
            int x = 960 + (int) (side * (185 + z * 880));
            int base = 678 + (int) (z * z * 490);
            int width = (int) (42 * scale), height = (int) ((124 + (i % 4) * 28) * scale);
            g.setColor(new Color(14, 18, 27));
            g.fillRect(x, base - height, width, height);
            g.setColor(i % 3 == 0 ? AMBER : CYAN);
            g.fillRect(x, base - height, Math.max(2, width / 10), height);
            g.setColor(new Color(255, 255, 255, 95));
            g.fillRect(x, base - height, width, 2);
        }

        // Drifting atmospheric particles.
        Random random = new Random(21);
        for (int p = 0; p < 110; p++) {
            double start = random.nextDouble(), speed = 1 + random.nextInt(3);
            int x = (int) ((random.nextDouble() * 1920 + t * 192 * speed) % 1920);
            int y = (int) (380 + (start * 640 + 55 * Math.sin(phase + p)) % 640);
            g.setColor(p % 3 == 0 ? new Color(255, 200, 102, 70 + p % 80) : new Color(148, 218, 252, 60 + p % 75));
            g.fillOval(x, y, 2 + p % 3, 2 + p % 3);
        }

        // Sleek aerospace interceptor centered in mid-lower flight corridor.
        int shipX = 960 + (int) Math.round(Math.sin(phase) * 165);
        int shipY = 748 + (int) Math.round(Math.sin(phase * 2) * 20);
        glow(g, shipX, shipY + 34, 145, new Color(245, 176, 65, 130));
        g.setColor(new Color(255, 200, 102, 205));
        g.fill(new Polygon(
                new int[] {shipX - 32, shipX, shipX + 32},
                new int[] {shipY + 18, shipY + 138 + (int) (Math.sin(phase * 12) * 16), shipY + 18}, 3));
        g.setColor(new Color(18, 22, 32));
        Polygon hull = new Polygon(
                new int[] {shipX, shipX + 116, shipX + 34, shipX, shipX - 34, shipX - 116},
                new int[] {shipY - 96, shipY + 34, shipY + 18, shipY + 42, shipY + 18, shipY + 34}, 6);
        g.fill(hull);
        g.setColor(new Color(199, 212, 232));
        g.setStroke(new BasicStroke(2.4f));
        g.draw(hull);
        g.setColor(AMBER_HI);
        g.fill(new Polygon(new int[] {shipX, shipX + 18, shipX - 18}, new int[] {shipY - 64, shipY + 8, shipY + 8}, 3));
        g.setColor(CYAN);
        g.fillOval(shipX - 116, shipY + 30, 6, 6);
        g.fillOval(shipX + 110, shipY + 30, 6, 6);

        drawDiegeticHud(g, phase);
    }

    public static void main(String[] args) throws Exception {
        Path assets = Path.of(args[0]);
        initFonts(assets);
        Process encoder = new ProcessBuilder(args[1], "-y", "-hide_banner", "-loglevel", "warning",
                "-f", "rawvideo", "-pixel_format", "bgr24", "-video_size", "1920x1080", "-framerate", "60",
                "-i", "pipe:0", "-an", "-c:v", "libx264", "-preset", "fast", "-crf", "19", "-threads", "2",
                "-pix_fmt", "yuv420p", "-movflags", "+faststart", assets.resolve("stream-h264.mp4").toString())
                .redirectError(ProcessBuilder.Redirect.INHERIT).start();
        BufferedImage background = background();
        BufferedImage image = new BufferedImage(1920, 1080, BufferedImage.TYPE_3BYTE_BGR);
        try (OutputStream pipe = new BufferedOutputStream(encoder.getOutputStream(), 1024 * 1024)) {
            for (int f = 0; f < 600; f++) {
                Graphics2D g = graphics(image);
                frame(g, background, f);
                g.dispose();
                if (f == 150) ImageIO.write(image, "png", Path.of(args[2]).toFile());
                pipe.write(((DataBufferByte) image.getRaster().getDataBuffer()).getData());
                if (f % 120 == 0) System.out.println("Rendered " + f + " / 600 frames");
            }
        }
        if (encoder.waitFor() != 0) throw new IOException("FFmpeg failed");
    }
}
