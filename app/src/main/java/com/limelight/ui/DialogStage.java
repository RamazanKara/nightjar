package com.limelight.ui;

import android.app.Activity;
import android.app.Dialog;
import android.content.Context;
import android.content.ContextWrapper;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.widget.FrameLayout;

import java.util.HashMap;
import java.util.Map;

/**
 * A dialog on its own surface ({@link NavigationDialogBuilder#ownSurface}): while it shows, the stage covers its page, so the
 * panel never stands over the page's heading or a card's edge (a TV's pairing PIN stood over the PC's name). The stage draws
 * once and never animates, and the page under it is out of a screen reader's reach until the dialog goes.
 */
final class DialogStage {
    private DialogStage() {
    }

    static void under(Dialog dialog) {
        Activity activity = activityOf(dialog.getContext());
        Window window = dialog.getWindow();
        if (activity == null || window == null) return;
        ViewGroup content = activity.findViewById(android.R.id.content);
        if (content == null) return;
        StageView stage = new StageView(activity);
        stage.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        Map<View, Integer> hidden = new HashMap<>();
        window.getDecorView().addOnAttachStateChangeListener(new View.OnAttachStateChangeListener() {
            @Override public void onViewAttachedToWindow(View decor) {
                if (stage.getParent() != null) return;
                for (int i = 0; i < content.getChildCount(); i++) {
                    View page = content.getChildAt(i);
                    hidden.put(page, page.getImportantForAccessibility());
                    page.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS);
                }
                content.addView(stage, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
            }

            @Override public void onViewDetachedFromWindow(View decor) {
                content.removeView(stage);
                for (Map.Entry<View, Integer> page : hidden.entrySet()) page.getKey().setImportantForAccessibility(page.getValue());
                hidden.clear();
            }
        });
    }

    private static Activity activityOf(Context context) {
        while (context instanceof ContextWrapper) {
            if (context instanceof Activity) return (Activity) context;
            context = ((ContextWrapper) context).getBaseContext();
        }
        return null;
    }
}
