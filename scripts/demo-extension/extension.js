// Loaded only into the headless GNOME Shell that scripts/demo.py records.
// The recording's input session counts as screen sharing, so the Shell would
// show its indicator in the top bar; this hides it. It also tells the script
// where windows, labels and notification banners are, so the script can
// click them and zoom in on them.

import Gio from 'gi://Gio';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const IFACE = `<node>
  <interface name="io.github.cszach.CalliopeDemo">
    <method name="Windows"><arg type="s" direction="out"/></method>
    <method name="Find">
      <arg type="s" direction="in"/>
      <arg type="s" direction="out"/>
    </method>
    <method name="Banner"><arg type="s" direction="out"/></method>
  </interface>
</node>`;

function rect(actor) {
    const box = actor.get_transformed_extents();
    return {x: box.origin.x, y: box.origin.y, width: box.size.width, height: box.size.height};
}

function* descendants(actor) {
    for (const child of actor.get_children()) {
        if (!child.visible)
            continue;
        yield child;
        yield* descendants(child);
    }
}

export default class DemoExtension extends Extension {
    enable() {
        Main.panel.statusArea.screenSharing.container.hide();
        this._object = Gio.DBusExportedObject.wrapJSObject(IFACE, this);
        this._object.export(Gio.DBus.session, '/io/github/cszach/CalliopeDemo');
        this._name = Gio.bus_own_name(Gio.BusType.SESSION,
            'io.github.cszach.CalliopeDemo', Gio.BusNameOwnerFlags.NONE,
            null, null, null);
    }

    disable() {
        Gio.bus_unown_name(this._name);
        this._object.unexport();
        Main.panel.statusArea.screenSharing.container.show();
    }

    // Every normal window, in stacking order, in logical pixels.
    Windows() {
        const focus = global.display.focus_window;
        return JSON.stringify(global.get_window_actors()
            .map(actor => actor.meta_window)
            .filter(w => !w.skip_taskbar)
            .map(w => {
                const {x, y, width, height} = w.get_frame_rect();
                return {
                    app: w.get_gtk_application_id() ?? w.get_wm_class(),
                    title: w.get_title(),
                    focused: w === focus,
                    x, y, width, height,
                };
            }));
    }

    // The clickable widget around the first label on screen with this text,
    // or null.
    Find(text) {
        for (const actor of descendants(global.stage)) {
            if (!(actor instanceof St.Label) || actor.text !== text || !actor.mapped)
                continue;
            let target = actor;
            while (target.get_parent() && !target.reactive)
                target = target.get_parent();
            return JSON.stringify(rect(target));
        }
        return 'null';
    }

    // The notification banner on screen, or null.
    Banner() {
        const bin = Main.messageTray._bannerBin;
        const banner = bin?.get_first_child();
        return JSON.stringify(banner?.mapped ? rect(banner) : null);
    }
}
