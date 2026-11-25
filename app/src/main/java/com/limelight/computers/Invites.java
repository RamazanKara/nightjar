package com.limelight.computers;

import android.content.Context;
import android.os.Build;
import android.preference.PreferenceManager;
import android.util.Base64;

import com.limelight.R;
import com.limelight.binding.PlatformBinding;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.InviteLink;
import com.limelight.nvstream.http.LimelightCryptoProvider;
import com.limelight.nvstream.jni.NightjarHostsJni;
import com.limelight.nvstream.jni.NightjarPolicyJni;
import com.limelight.preferences.PreferenceConfiguration;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;
import java.security.cert.CertificateException;
import java.security.cert.CertificateFactory;
import java.security.cert.X509Certificate;

/**
 * Opening an invite to play over the internet (docs/multiclient.md in the host repo): libnightjar_core reaches the PC
 * with the link's rendezvous secret, pairs this device as a guest with the link's PIN while the PC's certificate must
 * match the link, and fetches this device's own internet play details ({@link InternetPlay#redeem}); the PC is then
 * kept here like any paired PC, with those details.
 */
public final class Invites {
    /** The engine's words for an invite that did not work (nightjar-core invite.rs), translated here. */
    static final String NOT_AN_INVITE = "This is not a Nightjar invite link";
    static final String EXPIRED = "This invite has expired. Ask for a new one.";
    static final String REFUSED = "The PC did not accept this invite: it was already used, taken back or has expired. Ask for a new one.";
    static final String INTERNET_OFF = "Turn on Play over the internet in Nightjar's settings, then open the invite again.";
    /** This app's own: the invite names a PC known here that is paired with another certificate than the invite's. */
    static final String KNOWN_PC = "This invite is for a PC that is already on your list with another certificate.";

    private Invites() {
    }

    /** The words for an engine error, in the app's language where it is one of the known ones. */
    public static String errorText(Context context, String english) {
        if (english == null) return context.getString(R.string.invite_failed_title);
        switch (NightjarPolicyJni.classifyInviteError(english)) {
            case 0: return context.getString(R.string.invite_not_a_link);
            case 1: return context.getString(R.string.invite_expired);
            case 2: return context.getString(R.string.invite_refused);
            case 3: return context.getString(R.string.invite_internet_off);
            case 4: return context.getString(R.string.invite_known_pc);
            default: return english;
        }
    }

    /** The PC's name an invite shows, or words for "the PC" when it names none. */
    public static String shownName(Context context, InviteLink invite) {
        String name = invite == null ? "" : invite.shownName();
        return name.isEmpty() ? context.getString(R.string.invite_this_pc) : name;
    }

    /** What the friend may do, in the words the Guests menu uses. */
    public static String permissionLabel(Context context, String permission) {
        if (InviteLink.VIEW.equals(permission)) return context.getString(R.string.guests_view);
        if (InviteLink.KEYBOARD_MOUSE.equals(permission)) return context.getString(R.string.guests_keyboard_mouse);
        return context.getString(R.string.guests_controller);
    }

    /** The PC the engine paired, as this app keeps one: its ID, name, the address that answered and its certificate. */
    static ComputerDetails details(JSONObject redeemed) throws JSONException, CertificateException {
        ComputerDetails details = new ComputerDetails();
        details.uuid = redeemed.getString("uniqueId");
        details.name = redeemed.getString("name");
        ComputerDetails.AddressTuple answered =
                new ComputerDetails.AddressTuple(redeemed.getString("address"), redeemed.getInt("port"));
        // A punched path's local end lasts only as long as its tunnel: polls find it again from the kept reply.
        if (!NightjarPolicyJni.isTunnelAddress(answered.address)) details.manualAddress = answered;
        details.activeAddress = answered;
        details.httpsPort = redeemed.optInt("httpsPort");
        details.serverCert = certificate(redeemed.getString("certificate"));
        details.state = ComputerDetails.State.UNKNOWN;
        return details;
    }

    static X509Certificate certificate(String pem) throws CertificateException {
        return (X509Certificate) CertificateFactory.getInstance("X.509")
                .generateCertificate(new ByteArrayInputStream(pem.getBytes(StandardCharsets.US_ASCII)));
    }

    static String pem(String label, byte[] der) {
        String base64 = Base64.encodeToString(der, Base64.NO_WRAP);
        StringBuilder pem = new StringBuilder("-----BEGIN ").append(label).append("-----\n");
        for (int i = 0; i < base64.length(); i += 64) {
            pem.append(base64, i, Math.min(i + 64, base64.length())).append('\n');
        }
        return pem.append("-----END ").append(label).append("-----\n").toString();
    }

    /**
     * Opens {@code link} and keeps the PC: blocks while the PC is looked for over the internet (up to about a minute)
     * and paired. Returns the PC as kept; throws with the engine's words otherwise.
     */
    public static ComputerDetails redeem(Context context, String link, ComputerManagerService.ComputerManagerBinder binder)
            throws InternetPlay.Unreachable {
        // Before anything is sent, so the invite is not spent either: the PC it names is known here with another
        // certificate, which the invite's PC cannot be.
        InviteLink invite = InviteLink.parse(link);
        if (invite != null && binder.inviteClaimsKnownPc(invite)) {
            throw new InternetPlay.Unreachable(KNOWN_PC);
        }
        LimelightCryptoProvider crypto = PlatformBinding.getCryptoProvider(context);
        if (crypto == null || crypto.getClientCertificate() == null || crypto.getClientPrivateKey() == null) {
            throw new InternetPlay.Unreachable("Client identity certificate or private key is missing");
        }
        // The engine refuses with its words when this device's "Play over the internet" is off.
        InternetPlay.setEnabled(PreferenceManager.getDefaultSharedPreferences(context)
                .getBoolean(PreferenceConfiguration.INTERNET_PLAY_PREF_STRING, true));
        try {
            String raw = NightjarHostsJni.hostFlowRedeemInvite(
                    link,
                    binder.getUniqueId(),
                    pem("CERTIFICATE", crypto.getClientCertificate().getEncoded()),
                    pem("PRIVATE KEY", crypto.getClientPrivateKey().getEncoded()),
                    Build.MODEL,
                    System.currentTimeMillis() / 1000);
            JSONObject envelope = new JSONObject(raw);
            if (envelope.has("error")) {
                throw new InternetPlay.Unreachable(envelope.optString("error"));
            }
            JSONObject redeemed = envelope.getJSONObject("ok");
            ComputerDetails details = details(redeemed);
            if (!binder.addInvitedComputer(details, redeemed.getString("reply"))) {
                throw new InternetPlay.Unreachable(KNOWN_PC);
            }
            return details;
        } catch (JSONException | CertificateException e) {
            throw new InternetPlay.Unreachable(String.valueOf(e.getMessage()));
        }
    }
}
