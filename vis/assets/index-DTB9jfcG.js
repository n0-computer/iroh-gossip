var __getOwnPropNames = Object.getOwnPropertyNames;
var __commonJS = (cb, mod) => function __require() {
  return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
};
var require_stdin = __commonJS({
  "<stdin>"(exports, module) {
    (async () => {
      function xb(l, a) {
        for (var o = 0; o < a.length; o++) {
          const i = a[o];
          if (typeof i != "string" && !Array.isArray(i)) {
            for (const u in i) if (u !== "default" && !(u in l)) {
              const c = Object.getOwnPropertyDescriptor(i, u);
              c && Object.defineProperty(l, u, c.get ? c : {
                enumerable: true,
                get: () => i[u]
              });
            }
          }
        }
        return Object.freeze(Object.defineProperty(l, Symbol.toStringTag, {
          value: "Module"
        }));
      }
      (function() {
        const a = document.createElement("link").relList;
        if (a && a.supports && a.supports("modulepreload")) return;
        for (const u of document.querySelectorAll('link[rel="modulepreload"]')) i(u);
        new MutationObserver((u) => {
          for (const c of u) if (c.type === "childList") for (const d of c.addedNodes) d.tagName === "LINK" && d.rel === "modulepreload" && i(d);
        }).observe(document, {
          childList: true,
          subtree: true
        });
        function o(u) {
          const c = {};
          return u.integrity && (c.integrity = u.integrity), u.referrerPolicy && (c.referrerPolicy = u.referrerPolicy), u.crossOrigin === "use-credentials" ? c.credentials = "include" : u.crossOrigin === "anonymous" ? c.credentials = "omit" : c.credentials = "same-origin", c;
        }
        function i(u) {
          if (u.ep) return;
          u.ep = true;
          const c = o(u);
          fetch(u.href, c);
        }
      })();
      function Lc(l) {
        return l && l.__esModule && Object.prototype.hasOwnProperty.call(l, "default") ? l.default : l;
      }
      var Xs = {
        exports: {}
      }, Lr = {};
      var qg;
      function Tb() {
        if (qg) return Lr;
        qg = 1;
        var l = Symbol.for("react.transitional.element"), a = Symbol.for("react.fragment");
        function o(i, u, c) {
          var d = null;
          if (c !== void 0 && (d = "" + c), u.key !== void 0 && (d = "" + u.key), "key" in u) {
            c = {};
            for (var h in u) h !== "key" && (c[h] = u[h]);
          } else c = u;
          return u = c.ref, {
            $$typeof: l,
            type: i,
            key: d,
            ref: u !== void 0 ? u : null,
            props: c
          };
        }
        return Lr.Fragment = a, Lr.jsx = o, Lr.jsxs = o, Lr;
      }
      var Vg;
      function Ab() {
        return Vg || (Vg = 1, Xs.exports = Tb()), Xs.exports;
      }
      var V = Ab(), Zs = {
        exports: {}
      }, Re = {};
      var Yg;
      function Rb() {
        if (Yg) return Re;
        Yg = 1;
        var l = Symbol.for("react.transitional.element"), a = Symbol.for("react.portal"), o = Symbol.for("react.fragment"), i = Symbol.for("react.strict_mode"), u = Symbol.for("react.profiler"), c = Symbol.for("react.consumer"), d = Symbol.for("react.context"), h = Symbol.for("react.forward_ref"), m = Symbol.for("react.suspense"), g = Symbol.for("react.memo"), p = Symbol.for("react.lazy"), _ = Symbol.iterator;
        function E(x) {
          return x === null || typeof x != "object" ? null : (x = _ && x[_] || x["@@iterator"], typeof x == "function" ? x : null);
        }
        var R = {
          isMounted: function() {
            return false;
          },
          enqueueForceUpdate: function() {
          },
          enqueueReplaceState: function() {
          },
          enqueueSetState: function() {
          }
        }, z = Object.assign, H = {};
        function Y(x, k, K) {
          this.props = x, this.context = k, this.refs = H, this.updater = K || R;
        }
        Y.prototype.isReactComponent = {}, Y.prototype.setState = function(x, k) {
          if (typeof x != "object" && typeof x != "function" && x != null) throw Error("takes an object of state variables to update or a function which returns an object of state variables.");
          this.updater.enqueueSetState(this, x, k, "setState");
        }, Y.prototype.forceUpdate = function(x) {
          this.updater.enqueueForceUpdate(this, x, "forceUpdate");
        };
        function te() {
        }
        te.prototype = Y.prototype;
        function J(x, k, K) {
          this.props = x, this.context = k, this.refs = H, this.updater = K || R;
        }
        var I = J.prototype = new te();
        I.constructor = J, z(I, Y.prototype), I.isPureReactComponent = true;
        var G = Array.isArray, B = {
          H: null,
          A: null,
          T: null,
          S: null,
          V: null
        }, Z = Object.prototype.hasOwnProperty;
        function T(x, k, K, U, ie, de) {
          return K = de.ref, {
            $$typeof: l,
            type: x,
            key: k,
            ref: K !== void 0 ? K : null,
            props: de
          };
        }
        function y(x, k) {
          return T(x.type, k, void 0, void 0, void 0, x.props);
        }
        function w(x) {
          return typeof x == "object" && x !== null && x.$$typeof === l;
        }
        function S(x) {
          var k = {
            "=": "=0",
            ":": "=2"
          };
          return "$" + x.replace(/[=:]/g, function(K) {
            return k[K];
          });
        }
        var N = /\/+/g;
        function L(x, k) {
          return typeof x == "object" && x !== null && x.key != null ? S("" + x.key) : k.toString(36);
        }
        function re() {
        }
        function ue(x) {
          switch (x.status) {
            case "fulfilled":
              return x.value;
            case "rejected":
              throw x.reason;
            default:
              switch (typeof x.status == "string" ? x.then(re, re) : (x.status = "pending", x.then(function(k) {
                x.status === "pending" && (x.status = "fulfilled", x.value = k);
              }, function(k) {
                x.status === "pending" && (x.status = "rejected", x.reason = k);
              })), x.status) {
                case "fulfilled":
                  return x.value;
                case "rejected":
                  throw x.reason;
              }
          }
          throw x;
        }
        function oe(x, k, K, U, ie) {
          var de = typeof x;
          (de === "undefined" || de === "boolean") && (x = null);
          var ce = false;
          if (x === null) ce = true;
          else switch (de) {
            case "bigint":
            case "string":
            case "number":
              ce = true;
              break;
            case "object":
              switch (x.$$typeof) {
                case l:
                case a:
                  ce = true;
                  break;
                case p:
                  return ce = x._init, oe(ce(x._payload), k, K, U, ie);
              }
          }
          if (ce) return ie = ie(x), ce = U === "" ? "." + L(x, 0) : U, G(ie) ? (K = "", ce != null && (K = ce.replace(N, "$&/") + "/"), oe(ie, k, K, "", function(_e) {
            return _e;
          })) : ie != null && (w(ie) && (ie = y(ie, K + (ie.key == null || x && x.key === ie.key ? "" : ("" + ie.key).replace(N, "$&/") + "/") + ce)), k.push(ie)), 1;
          ce = 0;
          var be = U === "" ? "." : U + ":";
          if (G(x)) for (var fe = 0; fe < x.length; fe++) U = x[fe], de = be + L(U, fe), ce += oe(U, k, K, de, ie);
          else if (fe = E(x), typeof fe == "function") for (x = fe.call(x), fe = 0; !(U = x.next()).done; ) U = U.value, de = be + L(U, fe++), ce += oe(U, k, K, de, ie);
          else if (de === "object") {
            if (typeof x.then == "function") return oe(ue(x), k, K, U, ie);
            throw k = String(x), Error("Objects are not valid as a React child (found: " + (k === "[object Object]" ? "object with keys {" + Object.keys(x).join(", ") + "}" : k) + "). If you meant to render a collection of children, use an array instead.");
          }
          return ce;
        }
        function D(x, k, K) {
          if (x == null) return x;
          var U = [], ie = 0;
          return oe(x, U, "", "", function(de) {
            return k.call(K, de, ie++);
          }), U;
        }
        function X(x) {
          if (x._status === -1) {
            var k = x._result;
            k = k(), k.then(function(K) {
              (x._status === 0 || x._status === -1) && (x._status = 1, x._result = K);
            }, function(K) {
              (x._status === 0 || x._status === -1) && (x._status = 2, x._result = K);
            }), x._status === -1 && (x._status = 0, x._result = k);
          }
          if (x._status === 1) return x._result.default;
          throw x._result;
        }
        var $ = typeof reportError == "function" ? reportError : function(x) {
          if (typeof window == "object" && typeof window.ErrorEvent == "function") {
            var k = new window.ErrorEvent("error", {
              bubbles: true,
              cancelable: true,
              message: typeof x == "object" && x !== null && typeof x.message == "string" ? String(x.message) : String(x),
              error: x
            });
            if (!window.dispatchEvent(k)) return;
          } else if (typeof process == "object" && typeof process.emit == "function") {
            process.emit("uncaughtException", x);
            return;
          }
          console.error(x);
        };
        function ne() {
        }
        return Re.Children = {
          map: D,
          forEach: function(x, k, K) {
            D(x, function() {
              k.apply(this, arguments);
            }, K);
          },
          count: function(x) {
            var k = 0;
            return D(x, function() {
              k++;
            }), k;
          },
          toArray: function(x) {
            return D(x, function(k) {
              return k;
            }) || [];
          },
          only: function(x) {
            if (!w(x)) throw Error("React.Children.only expected to receive a single React element child.");
            return x;
          }
        }, Re.Component = Y, Re.Fragment = o, Re.Profiler = u, Re.PureComponent = J, Re.StrictMode = i, Re.Suspense = m, Re.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = B, Re.__COMPILER_RUNTIME = {
          __proto__: null,
          c: function(x) {
            return B.H.useMemoCache(x);
          }
        }, Re.cache = function(x) {
          return function() {
            return x.apply(null, arguments);
          };
        }, Re.cloneElement = function(x, k, K) {
          if (x == null) throw Error("The argument must be a React element, but you passed " + x + ".");
          var U = z({}, x.props), ie = x.key, de = void 0;
          if (k != null) for (ce in k.ref !== void 0 && (de = void 0), k.key !== void 0 && (ie = "" + k.key), k) !Z.call(k, ce) || ce === "key" || ce === "__self" || ce === "__source" || ce === "ref" && k.ref === void 0 || (U[ce] = k[ce]);
          var ce = arguments.length - 2;
          if (ce === 1) U.children = K;
          else if (1 < ce) {
            for (var be = Array(ce), fe = 0; fe < ce; fe++) be[fe] = arguments[fe + 2];
            U.children = be;
          }
          return T(x.type, ie, void 0, void 0, de, U);
        }, Re.createContext = function(x) {
          return x = {
            $$typeof: d,
            _currentValue: x,
            _currentValue2: x,
            _threadCount: 0,
            Provider: null,
            Consumer: null
          }, x.Provider = x, x.Consumer = {
            $$typeof: c,
            _context: x
          }, x;
        }, Re.createElement = function(x, k, K) {
          var U, ie = {}, de = null;
          if (k != null) for (U in k.key !== void 0 && (de = "" + k.key), k) Z.call(k, U) && U !== "key" && U !== "__self" && U !== "__source" && (ie[U] = k[U]);
          var ce = arguments.length - 2;
          if (ce === 1) ie.children = K;
          else if (1 < ce) {
            for (var be = Array(ce), fe = 0; fe < ce; fe++) be[fe] = arguments[fe + 2];
            ie.children = be;
          }
          if (x && x.defaultProps) for (U in ce = x.defaultProps, ce) ie[U] === void 0 && (ie[U] = ce[U]);
          return T(x, de, void 0, void 0, null, ie);
        }, Re.createRef = function() {
          return {
            current: null
          };
        }, Re.forwardRef = function(x) {
          return {
            $$typeof: h,
            render: x
          };
        }, Re.isValidElement = w, Re.lazy = function(x) {
          return {
            $$typeof: p,
            _payload: {
              _status: -1,
              _result: x
            },
            _init: X
          };
        }, Re.memo = function(x, k) {
          return {
            $$typeof: g,
            type: x,
            compare: k === void 0 ? null : k
          };
        }, Re.startTransition = function(x) {
          var k = B.T, K = {};
          B.T = K;
          try {
            var U = x(), ie = B.S;
            ie !== null && ie(K, U), typeof U == "object" && U !== null && typeof U.then == "function" && U.then(ne, $);
          } catch (de) {
            $(de);
          } finally {
            B.T = k;
          }
        }, Re.unstable_useCacheRefresh = function() {
          return B.H.useCacheRefresh();
        }, Re.use = function(x) {
          return B.H.use(x);
        }, Re.useActionState = function(x, k, K) {
          return B.H.useActionState(x, k, K);
        }, Re.useCallback = function(x, k) {
          return B.H.useCallback(x, k);
        }, Re.useContext = function(x) {
          return B.H.useContext(x);
        }, Re.useDebugValue = function() {
        }, Re.useDeferredValue = function(x, k) {
          return B.H.useDeferredValue(x, k);
        }, Re.useEffect = function(x, k, K) {
          var U = B.H;
          if (typeof K == "function") throw Error("useEffect CRUD overload is not enabled in this build of React.");
          return U.useEffect(x, k);
        }, Re.useId = function() {
          return B.H.useId();
        }, Re.useImperativeHandle = function(x, k, K) {
          return B.H.useImperativeHandle(x, k, K);
        }, Re.useInsertionEffect = function(x, k) {
          return B.H.useInsertionEffect(x, k);
        }, Re.useLayoutEffect = function(x, k) {
          return B.H.useLayoutEffect(x, k);
        }, Re.useMemo = function(x, k) {
          return B.H.useMemo(x, k);
        }, Re.useOptimistic = function(x, k) {
          return B.H.useOptimistic(x, k);
        }, Re.useReducer = function(x, k, K) {
          return B.H.useReducer(x, k, K);
        }, Re.useRef = function(x) {
          return B.H.useRef(x);
        }, Re.useState = function(x) {
          return B.H.useState(x);
        }, Re.useSyncExternalStore = function(x, k, K) {
          return B.H.useSyncExternalStore(x, k, K);
        }, Re.useTransition = function() {
          return B.H.useTransition();
        }, Re.version = "19.1.0", Re;
      }
      var $g;
      function Gc() {
        return $g || ($g = 1, Zs.exports = Rb()), Zs.exports;
      }
      var ae = Gc();
      const xn = Lc(ae), Qm = xb({
        __proto__: null,
        default: xn
      }, [
        ae
      ]);
      var Qs = {
        exports: {}
      }, Gr = {}, Ks = {
        exports: {}
      }, Ps = {};
      var Xg;
      function Cb() {
        return Xg || (Xg = 1, function(l) {
          function a(D, X) {
            var $ = D.length;
            D.push(X);
            e: for (; 0 < $; ) {
              var ne = $ - 1 >>> 1, x = D[ne];
              if (0 < u(x, X)) D[ne] = X, D[$] = x, $ = ne;
              else break e;
            }
          }
          function o(D) {
            return D.length === 0 ? null : D[0];
          }
          function i(D) {
            if (D.length === 0) return null;
            var X = D[0], $ = D.pop();
            if ($ !== X) {
              D[0] = $;
              e: for (var ne = 0, x = D.length, k = x >>> 1; ne < k; ) {
                var K = 2 * (ne + 1) - 1, U = D[K], ie = K + 1, de = D[ie];
                if (0 > u(U, $)) ie < x && 0 > u(de, U) ? (D[ne] = de, D[ie] = $, ne = ie) : (D[ne] = U, D[K] = $, ne = K);
                else if (ie < x && 0 > u(de, $)) D[ne] = de, D[ie] = $, ne = ie;
                else break e;
              }
            }
            return X;
          }
          function u(D, X) {
            var $ = D.sortIndex - X.sortIndex;
            return $ !== 0 ? $ : D.id - X.id;
          }
          if (l.unstable_now = void 0, typeof performance == "object" && typeof performance.now == "function") {
            var c = performance;
            l.unstable_now = function() {
              return c.now();
            };
          } else {
            var d = Date, h = d.now();
            l.unstable_now = function() {
              return d.now() - h;
            };
          }
          var m = [], g = [], p = 1, _ = null, E = 3, R = false, z = false, H = false, Y = false, te = typeof setTimeout == "function" ? setTimeout : null, J = typeof clearTimeout == "function" ? clearTimeout : null, I = typeof setImmediate < "u" ? setImmediate : null;
          function G(D) {
            for (var X = o(g); X !== null; ) {
              if (X.callback === null) i(g);
              else if (X.startTime <= D) i(g), X.sortIndex = X.expirationTime, a(m, X);
              else break;
              X = o(g);
            }
          }
          function B(D) {
            if (H = false, G(D), !z) if (o(m) !== null) z = true, Z || (Z = true, L());
            else {
              var X = o(g);
              X !== null && oe(B, X.startTime - D);
            }
          }
          var Z = false, T = -1, y = 5, w = -1;
          function S() {
            return Y ? true : !(l.unstable_now() - w < y);
          }
          function N() {
            if (Y = false, Z) {
              var D = l.unstable_now();
              w = D;
              var X = true;
              try {
                e: {
                  z = false, H && (H = false, J(T), T = -1), R = true;
                  var $ = E;
                  try {
                    t: {
                      for (G(D), _ = o(m); _ !== null && !(_.expirationTime > D && S()); ) {
                        var ne = _.callback;
                        if (typeof ne == "function") {
                          _.callback = null, E = _.priorityLevel;
                          var x = ne(_.expirationTime <= D);
                          if (D = l.unstable_now(), typeof x == "function") {
                            _.callback = x, G(D), X = true;
                            break t;
                          }
                          _ === o(m) && i(m), G(D);
                        } else i(m);
                        _ = o(m);
                      }
                      if (_ !== null) X = true;
                      else {
                        var k = o(g);
                        k !== null && oe(B, k.startTime - D), X = false;
                      }
                    }
                    break e;
                  } finally {
                    _ = null, E = $, R = false;
                  }
                  X = void 0;
                }
              } finally {
                X ? L() : Z = false;
              }
            }
          }
          var L;
          if (typeof I == "function") L = function() {
            I(N);
          };
          else if (typeof MessageChannel < "u") {
            var re = new MessageChannel(), ue = re.port2;
            re.port1.onmessage = N, L = function() {
              ue.postMessage(null);
            };
          } else L = function() {
            te(N, 0);
          };
          function oe(D, X) {
            T = te(function() {
              D(l.unstable_now());
            }, X);
          }
          l.unstable_IdlePriority = 5, l.unstable_ImmediatePriority = 1, l.unstable_LowPriority = 4, l.unstable_NormalPriority = 3, l.unstable_Profiling = null, l.unstable_UserBlockingPriority = 2, l.unstable_cancelCallback = function(D) {
            D.callback = null;
          }, l.unstable_forceFrameRate = function(D) {
            0 > D || 125 < D ? console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported") : y = 0 < D ? Math.floor(1e3 / D) : 5;
          }, l.unstable_getCurrentPriorityLevel = function() {
            return E;
          }, l.unstable_next = function(D) {
            switch (E) {
              case 1:
              case 2:
              case 3:
                var X = 3;
                break;
              default:
                X = E;
            }
            var $ = E;
            E = X;
            try {
              return D();
            } finally {
              E = $;
            }
          }, l.unstable_requestPaint = function() {
            Y = true;
          }, l.unstable_runWithPriority = function(D, X) {
            switch (D) {
              case 1:
              case 2:
              case 3:
              case 4:
              case 5:
                break;
              default:
                D = 3;
            }
            var $ = E;
            E = D;
            try {
              return X();
            } finally {
              E = $;
            }
          }, l.unstable_scheduleCallback = function(D, X, $) {
            var ne = l.unstable_now();
            switch (typeof $ == "object" && $ !== null ? ($ = $.delay, $ = typeof $ == "number" && 0 < $ ? ne + $ : ne) : $ = ne, D) {
              case 1:
                var x = -1;
                break;
              case 2:
                x = 250;
                break;
              case 5:
                x = 1073741823;
                break;
              case 4:
                x = 1e4;
                break;
              default:
                x = 5e3;
            }
            return x = $ + x, D = {
              id: p++,
              callback: X,
              priorityLevel: D,
              startTime: $,
              expirationTime: x,
              sortIndex: -1
            }, $ > ne ? (D.sortIndex = $, a(g, D), o(m) === null && D === o(g) && (H ? (J(T), T = -1) : H = true, oe(B, $ - ne))) : (D.sortIndex = x, a(m, D), z || R || (z = true, Z || (Z = true, L()))), D;
          }, l.unstable_shouldYield = S, l.unstable_wrapCallback = function(D) {
            var X = E;
            return function() {
              var $ = E;
              E = X;
              try {
                return D.apply(this, arguments);
              } finally {
                E = $;
              }
            };
          };
        }(Ps)), Ps;
      }
      var Zg;
      function Db() {
        return Zg || (Zg = 1, Ks.exports = Cb()), Ks.exports;
      }
      var Is = {
        exports: {}
      }, Ct = {};
      var Qg;
      function Nb() {
        if (Qg) return Ct;
        Qg = 1;
        var l = Gc();
        function a(m) {
          var g = "https://react.dev/errors/" + m;
          if (1 < arguments.length) {
            g += "?args[]=" + encodeURIComponent(arguments[1]);
            for (var p = 2; p < arguments.length; p++) g += "&args[]=" + encodeURIComponent(arguments[p]);
          }
          return "Minified React error #" + m + "; visit " + g + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
        }
        function o() {
        }
        var i = {
          d: {
            f: o,
            r: function() {
              throw Error(a(522));
            },
            D: o,
            C: o,
            L: o,
            m: o,
            X: o,
            S: o,
            M: o
          },
          p: 0,
          findDOMNode: null
        }, u = Symbol.for("react.portal");
        function c(m, g, p) {
          var _ = 3 < arguments.length && arguments[3] !== void 0 ? arguments[3] : null;
          return {
            $$typeof: u,
            key: _ == null ? null : "" + _,
            children: m,
            containerInfo: g,
            implementation: p
          };
        }
        var d = l.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;
        function h(m, g) {
          if (m === "font") return "";
          if (typeof g == "string") return g === "use-credentials" ? g : "";
        }
        return Ct.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE = i, Ct.createPortal = function(m, g) {
          var p = 2 < arguments.length && arguments[2] !== void 0 ? arguments[2] : null;
          if (!g || g.nodeType !== 1 && g.nodeType !== 9 && g.nodeType !== 11) throw Error(a(299));
          return c(m, g, null, p);
        }, Ct.flushSync = function(m) {
          var g = d.T, p = i.p;
          try {
            if (d.T = null, i.p = 2, m) return m();
          } finally {
            d.T = g, i.p = p, i.d.f();
          }
        }, Ct.preconnect = function(m, g) {
          typeof m == "string" && (g ? (g = g.crossOrigin, g = typeof g == "string" ? g === "use-credentials" ? g : "" : void 0) : g = null, i.d.C(m, g));
        }, Ct.prefetchDNS = function(m) {
          typeof m == "string" && i.d.D(m);
        }, Ct.preinit = function(m, g) {
          if (typeof m == "string" && g && typeof g.as == "string") {
            var p = g.as, _ = h(p, g.crossOrigin), E = typeof g.integrity == "string" ? g.integrity : void 0, R = typeof g.fetchPriority == "string" ? g.fetchPriority : void 0;
            p === "style" ? i.d.S(m, typeof g.precedence == "string" ? g.precedence : void 0, {
              crossOrigin: _,
              integrity: E,
              fetchPriority: R
            }) : p === "script" && i.d.X(m, {
              crossOrigin: _,
              integrity: E,
              fetchPriority: R,
              nonce: typeof g.nonce == "string" ? g.nonce : void 0
            });
          }
        }, Ct.preinitModule = function(m, g) {
          if (typeof m == "string") if (typeof g == "object" && g !== null) {
            if (g.as == null || g.as === "script") {
              var p = h(g.as, g.crossOrigin);
              i.d.M(m, {
                crossOrigin: p,
                integrity: typeof g.integrity == "string" ? g.integrity : void 0,
                nonce: typeof g.nonce == "string" ? g.nonce : void 0
              });
            }
          } else g == null && i.d.M(m);
        }, Ct.preload = function(m, g) {
          if (typeof m == "string" && typeof g == "object" && g !== null && typeof g.as == "string") {
            var p = g.as, _ = h(p, g.crossOrigin);
            i.d.L(m, p, {
              crossOrigin: _,
              integrity: typeof g.integrity == "string" ? g.integrity : void 0,
              nonce: typeof g.nonce == "string" ? g.nonce : void 0,
              type: typeof g.type == "string" ? g.type : void 0,
              fetchPriority: typeof g.fetchPriority == "string" ? g.fetchPriority : void 0,
              referrerPolicy: typeof g.referrerPolicy == "string" ? g.referrerPolicy : void 0,
              imageSrcSet: typeof g.imageSrcSet == "string" ? g.imageSrcSet : void 0,
              imageSizes: typeof g.imageSizes == "string" ? g.imageSizes : void 0,
              media: typeof g.media == "string" ? g.media : void 0
            });
          }
        }, Ct.preloadModule = function(m, g) {
          if (typeof m == "string") if (g) {
            var p = h(g.as, g.crossOrigin);
            i.d.m(m, {
              as: typeof g.as == "string" && g.as !== "script" ? g.as : void 0,
              crossOrigin: p,
              integrity: typeof g.integrity == "string" ? g.integrity : void 0
            });
          } else i.d.m(m);
        }, Ct.requestFormReset = function(m) {
          i.d.r(m);
        }, Ct.unstable_batchedUpdates = function(m, g) {
          return m(g);
        }, Ct.useFormState = function(m, g, p) {
          return d.H.useFormState(m, g, p);
        }, Ct.useFormStatus = function() {
          return d.H.useHostTransitionStatus();
        }, Ct.version = "19.1.0", Ct;
      }
      var Kg;
      function Km() {
        if (Kg) return Is.exports;
        Kg = 1;
        function l() {
          if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function")) try {
            __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(l);
          } catch (a) {
            console.error(a);
          }
        }
        return l(), Is.exports = Nb(), Is.exports;
      }
      var Pg;
      function Ob() {
        if (Pg) return Gr;
        Pg = 1;
        var l = Db(), a = Gc(), o = Km();
        function i(e) {
          var t = "https://react.dev/errors/" + e;
          if (1 < arguments.length) {
            t += "?args[]=" + encodeURIComponent(arguments[1]);
            for (var n = 2; n < arguments.length; n++) t += "&args[]=" + encodeURIComponent(arguments[n]);
          }
          return "Minified React error #" + e + "; visit " + t + " for the full message or use the non-minified dev environment for full errors and additional helpful warnings.";
        }
        function u(e) {
          return !(!e || e.nodeType !== 1 && e.nodeType !== 9 && e.nodeType !== 11);
        }
        function c(e) {
          var t = e, n = e;
          if (e.alternate) for (; t.return; ) t = t.return;
          else {
            e = t;
            do
              t = e, (t.flags & 4098) !== 0 && (n = t.return), e = t.return;
            while (e);
          }
          return t.tag === 3 ? n : null;
        }
        function d(e) {
          if (e.tag === 13) {
            var t = e.memoizedState;
            if (t === null && (e = e.alternate, e !== null && (t = e.memoizedState)), t !== null) return t.dehydrated;
          }
          return null;
        }
        function h(e) {
          if (c(e) !== e) throw Error(i(188));
        }
        function m(e) {
          var t = e.alternate;
          if (!t) {
            if (t = c(e), t === null) throw Error(i(188));
            return t !== e ? null : e;
          }
          for (var n = e, r = t; ; ) {
            var s = n.return;
            if (s === null) break;
            var f = s.alternate;
            if (f === null) {
              if (r = s.return, r !== null) {
                n = r;
                continue;
              }
              break;
            }
            if (s.child === f.child) {
              for (f = s.child; f; ) {
                if (f === n) return h(s), e;
                if (f === r) return h(s), t;
                f = f.sibling;
              }
              throw Error(i(188));
            }
            if (n.return !== r.return) n = s, r = f;
            else {
              for (var v = false, b = s.child; b; ) {
                if (b === n) {
                  v = true, n = s, r = f;
                  break;
                }
                if (b === r) {
                  v = true, r = s, n = f;
                  break;
                }
                b = b.sibling;
              }
              if (!v) {
                for (b = f.child; b; ) {
                  if (b === n) {
                    v = true, n = f, r = s;
                    break;
                  }
                  if (b === r) {
                    v = true, r = f, n = s;
                    break;
                  }
                  b = b.sibling;
                }
                if (!v) throw Error(i(189));
              }
            }
            if (n.alternate !== r) throw Error(i(190));
          }
          if (n.tag !== 3) throw Error(i(188));
          return n.stateNode.current === n ? e : t;
        }
        function g(e) {
          var t = e.tag;
          if (t === 5 || t === 26 || t === 27 || t === 6) return e;
          for (e = e.child; e !== null; ) {
            if (t = g(e), t !== null) return t;
            e = e.sibling;
          }
          return null;
        }
        var p = Object.assign, _ = Symbol.for("react.element"), E = Symbol.for("react.transitional.element"), R = Symbol.for("react.portal"), z = Symbol.for("react.fragment"), H = Symbol.for("react.strict_mode"), Y = Symbol.for("react.profiler"), te = Symbol.for("react.provider"), J = Symbol.for("react.consumer"), I = Symbol.for("react.context"), G = Symbol.for("react.forward_ref"), B = Symbol.for("react.suspense"), Z = Symbol.for("react.suspense_list"), T = Symbol.for("react.memo"), y = Symbol.for("react.lazy"), w = Symbol.for("react.activity"), S = Symbol.for("react.memo_cache_sentinel"), N = Symbol.iterator;
        function L(e) {
          return e === null || typeof e != "object" ? null : (e = N && e[N] || e["@@iterator"], typeof e == "function" ? e : null);
        }
        var re = Symbol.for("react.client.reference");
        function ue(e) {
          if (e == null) return null;
          if (typeof e == "function") return e.$$typeof === re ? null : e.displayName || e.name || null;
          if (typeof e == "string") return e;
          switch (e) {
            case z:
              return "Fragment";
            case Y:
              return "Profiler";
            case H:
              return "StrictMode";
            case B:
              return "Suspense";
            case Z:
              return "SuspenseList";
            case w:
              return "Activity";
          }
          if (typeof e == "object") switch (e.$$typeof) {
            case R:
              return "Portal";
            case I:
              return (e.displayName || "Context") + ".Provider";
            case J:
              return (e._context.displayName || "Context") + ".Consumer";
            case G:
              var t = e.render;
              return e = e.displayName, e || (e = t.displayName || t.name || "", e = e !== "" ? "ForwardRef(" + e + ")" : "ForwardRef"), e;
            case T:
              return t = e.displayName || null, t !== null ? t : ue(e.type) || "Memo";
            case y:
              t = e._payload, e = e._init;
              try {
                return ue(e(t));
              } catch {
              }
          }
          return null;
        }
        var oe = Array.isArray, D = a.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, X = o.__DOM_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE, $ = {
          pending: false,
          data: null,
          method: null,
          action: null
        }, ne = [], x = -1;
        function k(e) {
          return {
            current: e
          };
        }
        function K(e) {
          0 > x || (e.current = ne[x], ne[x] = null, x--);
        }
        function U(e, t) {
          x++, ne[x] = e.current, e.current = t;
        }
        var ie = k(null), de = k(null), ce = k(null), be = k(null);
        function fe(e, t) {
          switch (U(ce, t), U(de, e), U(ie, null), t.nodeType) {
            case 9:
            case 11:
              e = (e = t.documentElement) && (e = e.namespaceURI) ? vg(e) : 0;
              break;
            default:
              if (e = t.tagName, t = t.namespaceURI) t = vg(t), e = pg(t, e);
              else switch (e) {
                case "svg":
                  e = 1;
                  break;
                case "math":
                  e = 2;
                  break;
                default:
                  e = 0;
              }
          }
          K(ie), U(ie, e);
        }
        function _e() {
          K(ie), K(de), K(ce);
        }
        function ke(e) {
          e.memoizedState !== null && U(be, e);
          var t = ie.current, n = pg(t, e.type);
          t !== n && (U(de, e), U(ie, n));
        }
        function pe(e) {
          de.current === e && (K(ie), K(de)), be.current === e && (K(be), Nr._currentValue = $);
        }
        var Me = Object.prototype.hasOwnProperty, Ze = l.unstable_scheduleCallback, Le = l.unstable_cancelCallback, je = l.unstable_shouldYield, Ae = l.unstable_requestPaint, nt = l.unstable_now, Te = l.unstable_getCurrentPriorityLevel, Ke = l.unstable_ImmediatePriority, un = l.unstable_UserBlockingPriority, sn = l.unstable_NormalPriority, $t = l.unstable_LowPriority, Xn = l.unstable_IdlePriority, mt = l.log, Xt = l.unstable_setDisableYieldValue, Je = null, Ve = null;
        function vt(e) {
          if (typeof mt == "function" && Xt(e), Ve && typeof Ve.setStrictMode == "function") try {
            Ve.setStrictMode(Je, e);
          } catch {
          }
        }
        var wt = Math.clz32 ? Math.clz32 : Zt, Ir = Math.log, Zn = Math.LN2;
        function Zt(e) {
          return e >>>= 0, e === 0 ? 32 : 31 - (Ir(e) / Zn | 0) | 0;
        }
        var yn = 256, bi = 4194304;
        function _i(e) {
          var t = e & 42;
          if (t !== 0) return t;
          switch (e & -e) {
            case 1:
              return 1;
            case 2:
              return 2;
            case 4:
              return 4;
            case 8:
              return 8;
            case 16:
              return 16;
            case 32:
              return 32;
            case 64:
              return 64;
            case 128:
              return 128;
            case 256:
            case 512:
            case 1024:
            case 2048:
            case 4096:
            case 8192:
            case 16384:
            case 32768:
            case 65536:
            case 131072:
            case 262144:
            case 524288:
            case 1048576:
            case 2097152:
              return e & 4194048;
            case 4194304:
            case 8388608:
            case 16777216:
            case 33554432:
              return e & 62914560;
            case 67108864:
              return 67108864;
            case 134217728:
              return 134217728;
            case 268435456:
              return 268435456;
            case 536870912:
              return 536870912;
            case 1073741824:
              return 0;
            default:
              return e;
          }
        }
        function Wr(e, t, n) {
          var r = e.pendingLanes;
          if (r === 0) return 0;
          var s = 0, f = e.suspendedLanes, v = e.pingedLanes;
          e = e.warmLanes;
          var b = r & 134217727;
          return b !== 0 ? (r = b & ~f, r !== 0 ? s = _i(r) : (v &= b, v !== 0 ? s = _i(v) : n || (n = b & ~e, n !== 0 && (s = _i(n))))) : (b = r & ~f, b !== 0 ? s = _i(b) : v !== 0 ? s = _i(v) : n || (n = r & ~e, n !== 0 && (s = _i(n)))), s === 0 ? 0 : t !== 0 && t !== s && (t & f) === 0 && (f = s & -s, n = t & -t, f >= n || f === 32 && (n & 4194048) !== 0) ? t : s;
        }
        function Ba(e, t) {
          return (e.pendingLanes & ~(e.suspendedLanes & ~e.pingedLanes) & t) === 0;
        }
        function fp(e, t) {
          switch (e) {
            case 1:
            case 2:
            case 4:
            case 8:
            case 64:
              return t + 250;
            case 16:
            case 32:
            case 128:
            case 256:
            case 512:
            case 1024:
            case 2048:
            case 4096:
            case 8192:
            case 16384:
            case 32768:
            case 65536:
            case 131072:
            case 262144:
            case 524288:
            case 1048576:
            case 2097152:
              return t + 5e3;
            case 4194304:
            case 8388608:
            case 16777216:
            case 33554432:
              return -1;
            case 67108864:
            case 134217728:
            case 268435456:
            case 536870912:
            case 1073741824:
              return -1;
            default:
              return -1;
          }
        }
        function Wc() {
          var e = yn;
          return yn <<= 1, (yn & 4194048) === 0 && (yn = 256), e;
        }
        function Jc() {
          var e = bi;
          return bi <<= 1, (bi & 62914560) === 0 && (bi = 4194304), e;
        }
        function Lo(e) {
          for (var t = [], n = 0; 31 > n; n++) t.push(e);
          return t;
        }
        function Ha(e, t) {
          e.pendingLanes |= t, t !== 268435456 && (e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0);
        }
        function dp(e, t, n, r, s, f) {
          var v = e.pendingLanes;
          e.pendingLanes = n, e.suspendedLanes = 0, e.pingedLanes = 0, e.warmLanes = 0, e.expiredLanes &= n, e.entangledLanes &= n, e.errorRecoveryDisabledLanes &= n, e.shellSuspendCounter = 0;
          var b = e.entanglements, A = e.expirationTimes, j = e.hiddenUpdates;
          for (n = v & ~n; 0 < n; ) {
            var Q = 31 - wt(n), W = 1 << Q;
            b[Q] = 0, A[Q] = -1;
            var F = j[Q];
            if (F !== null) for (j[Q] = null, Q = 0; Q < F.length; Q++) {
              var q = F[Q];
              q !== null && (q.lane &= -536870913);
            }
            n &= ~W;
          }
          r !== 0 && ef(e, r, 0), f !== 0 && s === 0 && e.tag !== 0 && (e.suspendedLanes |= f & ~(v & ~t));
        }
        function ef(e, t, n) {
          e.pendingLanes |= t, e.suspendedLanes &= ~t;
          var r = 31 - wt(t);
          e.entangledLanes |= t, e.entanglements[r] = e.entanglements[r] | 1073741824 | n & 4194090;
        }
        function tf(e, t) {
          var n = e.entangledLanes |= t;
          for (e = e.entanglements; n; ) {
            var r = 31 - wt(n), s = 1 << r;
            s & t | e[r] & t && (e[r] |= t), n &= ~s;
          }
        }
        function Go(e) {
          switch (e) {
            case 2:
              e = 1;
              break;
            case 8:
              e = 4;
              break;
            case 32:
              e = 16;
              break;
            case 256:
            case 512:
            case 1024:
            case 2048:
            case 4096:
            case 8192:
            case 16384:
            case 32768:
            case 65536:
            case 131072:
            case 262144:
            case 524288:
            case 1048576:
            case 2097152:
            case 4194304:
            case 8388608:
            case 16777216:
            case 33554432:
              e = 128;
              break;
            case 268435456:
              e = 134217728;
              break;
            default:
              e = 0;
          }
          return e;
        }
        function Uo(e) {
          return e &= -e, 2 < e ? 8 < e ? (e & 134217727) !== 0 ? 32 : 268435456 : 8 : 2;
        }
        function nf() {
          var e = X.p;
          return e !== 0 ? e : (e = window.event, e === void 0 ? 32 : Gg(e.type));
        }
        function hp(e, t) {
          var n = X.p;
          try {
            return X.p = e, t();
          } finally {
            X.p = n;
          }
        }
        var Qn = Math.random().toString(36).slice(2), At = "__reactFiber$" + Qn, zt = "__reactProps$" + Qn, Fi = "__reactContainer$" + Qn, jo = "__reactEvents$" + Qn, gp = "__reactListeners$" + Qn, mp = "__reactHandles$" + Qn, af = "__reactResources$" + Qn, Fa = "__reactMarker$" + Qn;
        function Bo(e) {
          delete e[At], delete e[zt], delete e[jo], delete e[gp], delete e[mp];
        }
        function qi(e) {
          var t = e[At];
          if (t) return t;
          for (var n = e.parentNode; n; ) {
            if (t = n[Fi] || n[At]) {
              if (n = t.alternate, t.child !== null || n !== null && n.child !== null) for (e = wg(e); e !== null; ) {
                if (n = e[At]) return n;
                e = wg(e);
              }
              return t;
            }
            e = n, n = e.parentNode;
          }
          return null;
        }
        function Vi(e) {
          if (e = e[At] || e[Fi]) {
            var t = e.tag;
            if (t === 5 || t === 6 || t === 13 || t === 26 || t === 27 || t === 3) return e;
          }
          return null;
        }
        function qa(e) {
          var t = e.tag;
          if (t === 5 || t === 26 || t === 27 || t === 6) return e.stateNode;
          throw Error(i(33));
        }
        function Yi(e) {
          var t = e[af];
          return t || (t = e[af] = {
            hoistableStyles: /* @__PURE__ */ new Map(),
            hoistableScripts: /* @__PURE__ */ new Map()
          }), t;
        }
        function pt(e) {
          e[Fa] = true;
        }
        var rf = /* @__PURE__ */ new Set(), lf = {};
        function wi(e, t) {
          $i(e, t), $i(e + "Capture", t);
        }
        function $i(e, t) {
          for (lf[e] = t, e = 0; e < t.length; e++) rf.add(t[e]);
        }
        var vp = RegExp("^[:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD][:A-Z_a-z\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF\\u200C-\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD\\-.0-9\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*$"), of = {}, uf = {};
        function pp(e) {
          return Me.call(uf, e) ? true : Me.call(of, e) ? false : vp.test(e) ? uf[e] = true : (of[e] = true, false);
        }
        function Jr(e, t, n) {
          if (pp(t)) if (n === null) e.removeAttribute(t);
          else {
            switch (typeof n) {
              case "undefined":
              case "function":
              case "symbol":
                e.removeAttribute(t);
                return;
              case "boolean":
                var r = t.toLowerCase().slice(0, 5);
                if (r !== "data-" && r !== "aria-") {
                  e.removeAttribute(t);
                  return;
                }
            }
            e.setAttribute(t, "" + n);
          }
        }
        function el(e, t, n) {
          if (n === null) e.removeAttribute(t);
          else {
            switch (typeof n) {
              case "undefined":
              case "function":
              case "symbol":
              case "boolean":
                e.removeAttribute(t);
                return;
            }
            e.setAttribute(t, "" + n);
          }
        }
        function An(e, t, n, r) {
          if (r === null) e.removeAttribute(n);
          else {
            switch (typeof r) {
              case "undefined":
              case "function":
              case "symbol":
              case "boolean":
                e.removeAttribute(n);
                return;
            }
            e.setAttributeNS(t, n, "" + r);
          }
        }
        var Ho, sf;
        function Xi(e) {
          if (Ho === void 0) try {
            throw Error();
          } catch (n) {
            var t = n.stack.trim().match(/\n( *(at )?)/);
            Ho = t && t[1] || "", sf = -1 < n.stack.indexOf(`
    at`) ? " (<anonymous>)" : -1 < n.stack.indexOf("@") ? "@unknown:0:0" : "";
          }
          return `
` + Ho + e + sf;
        }
        var Fo = false;
        function qo(e, t) {
          if (!e || Fo) return "";
          Fo = true;
          var n = Error.prepareStackTrace;
          Error.prepareStackTrace = void 0;
          try {
            var r = {
              DetermineComponentFrameRoot: function() {
                try {
                  if (t) {
                    var W = function() {
                      throw Error();
                    };
                    if (Object.defineProperty(W.prototype, "props", {
                      set: function() {
                        throw Error();
                      }
                    }), typeof Reflect == "object" && Reflect.construct) {
                      try {
                        Reflect.construct(W, []);
                      } catch (q) {
                        var F = q;
                      }
                      Reflect.construct(e, [], W);
                    } else {
                      try {
                        W.call();
                      } catch (q) {
                        F = q;
                      }
                      e.call(W.prototype);
                    }
                  } else {
                    try {
                      throw Error();
                    } catch (q) {
                      F = q;
                    }
                    (W = e()) && typeof W.catch == "function" && W.catch(function() {
                    });
                  }
                } catch (q) {
                  if (q && F && typeof q.stack == "string") return [
                    q.stack,
                    F.stack
                  ];
                }
                return [
                  null,
                  null
                ];
              }
            };
            r.DetermineComponentFrameRoot.displayName = "DetermineComponentFrameRoot";
            var s = Object.getOwnPropertyDescriptor(r.DetermineComponentFrameRoot, "name");
            s && s.configurable && Object.defineProperty(r.DetermineComponentFrameRoot, "name", {
              value: "DetermineComponentFrameRoot"
            });
            var f = r.DetermineComponentFrameRoot(), v = f[0], b = f[1];
            if (v && b) {
              var A = v.split(`
`), j = b.split(`
`);
              for (s = r = 0; r < A.length && !A[r].includes("DetermineComponentFrameRoot"); ) r++;
              for (; s < j.length && !j[s].includes("DetermineComponentFrameRoot"); ) s++;
              if (r === A.length || s === j.length) for (r = A.length - 1, s = j.length - 1; 1 <= r && 0 <= s && A[r] !== j[s]; ) s--;
              for (; 1 <= r && 0 <= s; r--, s--) if (A[r] !== j[s]) {
                if (r !== 1 || s !== 1) do
                  if (r--, s--, 0 > s || A[r] !== j[s]) {
                    var Q = `
` + A[r].replace(" at new ", " at ");
                    return e.displayName && Q.includes("<anonymous>") && (Q = Q.replace("<anonymous>", e.displayName)), Q;
                  }
                while (1 <= r && 0 <= s);
                break;
              }
            }
          } finally {
            Fo = false, Error.prepareStackTrace = n;
          }
          return (n = e ? e.displayName || e.name : "") ? Xi(n) : "";
        }
        function yp(e) {
          switch (e.tag) {
            case 26:
            case 27:
            case 5:
              return Xi(e.type);
            case 16:
              return Xi("Lazy");
            case 13:
              return Xi("Suspense");
            case 19:
              return Xi("SuspenseList");
            case 0:
            case 15:
              return qo(e.type, false);
            case 11:
              return qo(e.type.render, false);
            case 1:
              return qo(e.type, true);
            case 31:
              return Xi("Activity");
            default:
              return "";
          }
        }
        function cf(e) {
          try {
            var t = "";
            do
              t += yp(e), e = e.return;
            while (e);
            return t;
          } catch (n) {
            return `
Error generating stack: ` + n.message + `
` + n.stack;
          }
        }
        function Qt(e) {
          switch (typeof e) {
            case "bigint":
            case "boolean":
            case "number":
            case "string":
            case "undefined":
              return e;
            case "object":
              return e;
            default:
              return "";
          }
        }
        function ff(e) {
          var t = e.type;
          return (e = e.nodeName) && e.toLowerCase() === "input" && (t === "checkbox" || t === "radio");
        }
        function bp(e) {
          var t = ff(e) ? "checked" : "value", n = Object.getOwnPropertyDescriptor(e.constructor.prototype, t), r = "" + e[t];
          if (!e.hasOwnProperty(t) && typeof n < "u" && typeof n.get == "function" && typeof n.set == "function") {
            var s = n.get, f = n.set;
            return Object.defineProperty(e, t, {
              configurable: true,
              get: function() {
                return s.call(this);
              },
              set: function(v) {
                r = "" + v, f.call(this, v);
              }
            }), Object.defineProperty(e, t, {
              enumerable: n.enumerable
            }), {
              getValue: function() {
                return r;
              },
              setValue: function(v) {
                r = "" + v;
              },
              stopTracking: function() {
                e._valueTracker = null, delete e[t];
              }
            };
          }
        }
        function tl(e) {
          e._valueTracker || (e._valueTracker = bp(e));
        }
        function df(e) {
          if (!e) return false;
          var t = e._valueTracker;
          if (!t) return true;
          var n = t.getValue(), r = "";
          return e && (r = ff(e) ? e.checked ? "true" : "false" : e.value), e = r, e !== n ? (t.setValue(e), true) : false;
        }
        function nl(e) {
          if (e = e || (typeof document < "u" ? document : void 0), typeof e > "u") return null;
          try {
            return e.activeElement || e.body;
          } catch {
            return e.body;
          }
        }
        var _p = /[\n"\\]/g;
        function Kt(e) {
          return e.replace(_p, function(t) {
            return "\\" + t.charCodeAt(0).toString(16) + " ";
          });
        }
        function Vo(e, t, n, r, s, f, v, b) {
          e.name = "", v != null && typeof v != "function" && typeof v != "symbol" && typeof v != "boolean" ? e.type = v : e.removeAttribute("type"), t != null ? v === "number" ? (t === 0 && e.value === "" || e.value != t) && (e.value = "" + Qt(t)) : e.value !== "" + Qt(t) && (e.value = "" + Qt(t)) : v !== "submit" && v !== "reset" || e.removeAttribute("value"), t != null ? Yo(e, v, Qt(t)) : n != null ? Yo(e, v, Qt(n)) : r != null && e.removeAttribute("value"), s == null && f != null && (e.defaultChecked = !!f), s != null && (e.checked = s && typeof s != "function" && typeof s != "symbol"), b != null && typeof b != "function" && typeof b != "symbol" && typeof b != "boolean" ? e.name = "" + Qt(b) : e.removeAttribute("name");
        }
        function hf(e, t, n, r, s, f, v, b) {
          if (f != null && typeof f != "function" && typeof f != "symbol" && typeof f != "boolean" && (e.type = f), t != null || n != null) {
            if (!(f !== "submit" && f !== "reset" || t != null)) return;
            n = n != null ? "" + Qt(n) : "", t = t != null ? "" + Qt(t) : n, b || t === e.value || (e.value = t), e.defaultValue = t;
          }
          r = r ?? s, r = typeof r != "function" && typeof r != "symbol" && !!r, e.checked = b ? e.checked : !!r, e.defaultChecked = !!r, v != null && typeof v != "function" && typeof v != "symbol" && typeof v != "boolean" && (e.name = v);
        }
        function Yo(e, t, n) {
          t === "number" && nl(e.ownerDocument) === e || e.defaultValue === "" + n || (e.defaultValue = "" + n);
        }
        function Zi(e, t, n, r) {
          if (e = e.options, t) {
            t = {};
            for (var s = 0; s < n.length; s++) t["$" + n[s]] = true;
            for (n = 0; n < e.length; n++) s = t.hasOwnProperty("$" + e[n].value), e[n].selected !== s && (e[n].selected = s), s && r && (e[n].defaultSelected = true);
          } else {
            for (n = "" + Qt(n), t = null, s = 0; s < e.length; s++) {
              if (e[s].value === n) {
                e[s].selected = true, r && (e[s].defaultSelected = true);
                return;
              }
              t !== null || e[s].disabled || (t = e[s]);
            }
            t !== null && (t.selected = true);
          }
        }
        function gf(e, t, n) {
          if (t != null && (t = "" + Qt(t), t !== e.value && (e.value = t), n == null)) {
            e.defaultValue !== t && (e.defaultValue = t);
            return;
          }
          e.defaultValue = n != null ? "" + Qt(n) : "";
        }
        function mf(e, t, n, r) {
          if (t == null) {
            if (r != null) {
              if (n != null) throw Error(i(92));
              if (oe(r)) {
                if (1 < r.length) throw Error(i(93));
                r = r[0];
              }
              n = r;
            }
            n == null && (n = ""), t = n;
          }
          n = Qt(t), e.defaultValue = n, r = e.textContent, r === n && r !== "" && r !== null && (e.value = r);
        }
        function Qi(e, t) {
          if (t) {
            var n = e.firstChild;
            if (n && n === e.lastChild && n.nodeType === 3) {
              n.nodeValue = t;
              return;
            }
          }
          e.textContent = t;
        }
        var wp = new Set("animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp".split(" "));
        function vf(e, t, n) {
          var r = t.indexOf("--") === 0;
          n == null || typeof n == "boolean" || n === "" ? r ? e.setProperty(t, "") : t === "float" ? e.cssFloat = "" : e[t] = "" : r ? e.setProperty(t, n) : typeof n != "number" || n === 0 || wp.has(t) ? t === "float" ? e.cssFloat = n : e[t] = ("" + n).trim() : e[t] = n + "px";
        }
        function pf(e, t, n) {
          if (t != null && typeof t != "object") throw Error(i(62));
          if (e = e.style, n != null) {
            for (var r in n) !n.hasOwnProperty(r) || t != null && t.hasOwnProperty(r) || (r.indexOf("--") === 0 ? e.setProperty(r, "") : r === "float" ? e.cssFloat = "" : e[r] = "");
            for (var s in t) r = t[s], t.hasOwnProperty(s) && n[s] !== r && vf(e, s, r);
          } else for (var f in t) t.hasOwnProperty(f) && vf(e, f, t[f]);
        }
        function $o(e) {
          if (e.indexOf("-") === -1) return false;
          switch (e) {
            case "annotation-xml":
            case "color-profile":
            case "font-face":
            case "font-face-src":
            case "font-face-uri":
            case "font-face-format":
            case "font-face-name":
            case "missing-glyph":
              return false;
            default:
              return true;
          }
        }
        var Ep = /* @__PURE__ */ new Map([
          [
            "acceptCharset",
            "accept-charset"
          ],
          [
            "htmlFor",
            "for"
          ],
          [
            "httpEquiv",
            "http-equiv"
          ],
          [
            "crossOrigin",
            "crossorigin"
          ],
          [
            "accentHeight",
            "accent-height"
          ],
          [
            "alignmentBaseline",
            "alignment-baseline"
          ],
          [
            "arabicForm",
            "arabic-form"
          ],
          [
            "baselineShift",
            "baseline-shift"
          ],
          [
            "capHeight",
            "cap-height"
          ],
          [
            "clipPath",
            "clip-path"
          ],
          [
            "clipRule",
            "clip-rule"
          ],
          [
            "colorInterpolation",
            "color-interpolation"
          ],
          [
            "colorInterpolationFilters",
            "color-interpolation-filters"
          ],
          [
            "colorProfile",
            "color-profile"
          ],
          [
            "colorRendering",
            "color-rendering"
          ],
          [
            "dominantBaseline",
            "dominant-baseline"
          ],
          [
            "enableBackground",
            "enable-background"
          ],
          [
            "fillOpacity",
            "fill-opacity"
          ],
          [
            "fillRule",
            "fill-rule"
          ],
          [
            "floodColor",
            "flood-color"
          ],
          [
            "floodOpacity",
            "flood-opacity"
          ],
          [
            "fontFamily",
            "font-family"
          ],
          [
            "fontSize",
            "font-size"
          ],
          [
            "fontSizeAdjust",
            "font-size-adjust"
          ],
          [
            "fontStretch",
            "font-stretch"
          ],
          [
            "fontStyle",
            "font-style"
          ],
          [
            "fontVariant",
            "font-variant"
          ],
          [
            "fontWeight",
            "font-weight"
          ],
          [
            "glyphName",
            "glyph-name"
          ],
          [
            "glyphOrientationHorizontal",
            "glyph-orientation-horizontal"
          ],
          [
            "glyphOrientationVertical",
            "glyph-orientation-vertical"
          ],
          [
            "horizAdvX",
            "horiz-adv-x"
          ],
          [
            "horizOriginX",
            "horiz-origin-x"
          ],
          [
            "imageRendering",
            "image-rendering"
          ],
          [
            "letterSpacing",
            "letter-spacing"
          ],
          [
            "lightingColor",
            "lighting-color"
          ],
          [
            "markerEnd",
            "marker-end"
          ],
          [
            "markerMid",
            "marker-mid"
          ],
          [
            "markerStart",
            "marker-start"
          ],
          [
            "overlinePosition",
            "overline-position"
          ],
          [
            "overlineThickness",
            "overline-thickness"
          ],
          [
            "paintOrder",
            "paint-order"
          ],
          [
            "panose-1",
            "panose-1"
          ],
          [
            "pointerEvents",
            "pointer-events"
          ],
          [
            "renderingIntent",
            "rendering-intent"
          ],
          [
            "shapeRendering",
            "shape-rendering"
          ],
          [
            "stopColor",
            "stop-color"
          ],
          [
            "stopOpacity",
            "stop-opacity"
          ],
          [
            "strikethroughPosition",
            "strikethrough-position"
          ],
          [
            "strikethroughThickness",
            "strikethrough-thickness"
          ],
          [
            "strokeDasharray",
            "stroke-dasharray"
          ],
          [
            "strokeDashoffset",
            "stroke-dashoffset"
          ],
          [
            "strokeLinecap",
            "stroke-linecap"
          ],
          [
            "strokeLinejoin",
            "stroke-linejoin"
          ],
          [
            "strokeMiterlimit",
            "stroke-miterlimit"
          ],
          [
            "strokeOpacity",
            "stroke-opacity"
          ],
          [
            "strokeWidth",
            "stroke-width"
          ],
          [
            "textAnchor",
            "text-anchor"
          ],
          [
            "textDecoration",
            "text-decoration"
          ],
          [
            "textRendering",
            "text-rendering"
          ],
          [
            "transformOrigin",
            "transform-origin"
          ],
          [
            "underlinePosition",
            "underline-position"
          ],
          [
            "underlineThickness",
            "underline-thickness"
          ],
          [
            "unicodeBidi",
            "unicode-bidi"
          ],
          [
            "unicodeRange",
            "unicode-range"
          ],
          [
            "unitsPerEm",
            "units-per-em"
          ],
          [
            "vAlphabetic",
            "v-alphabetic"
          ],
          [
            "vHanging",
            "v-hanging"
          ],
          [
            "vIdeographic",
            "v-ideographic"
          ],
          [
            "vMathematical",
            "v-mathematical"
          ],
          [
            "vectorEffect",
            "vector-effect"
          ],
          [
            "vertAdvY",
            "vert-adv-y"
          ],
          [
            "vertOriginX",
            "vert-origin-x"
          ],
          [
            "vertOriginY",
            "vert-origin-y"
          ],
          [
            "wordSpacing",
            "word-spacing"
          ],
          [
            "writingMode",
            "writing-mode"
          ],
          [
            "xmlnsXlink",
            "xmlns:xlink"
          ],
          [
            "xHeight",
            "x-height"
          ]
        ]), Sp = /^[\u0000-\u001F ]*j[\r\n\t]*a[\r\n\t]*v[\r\n\t]*a[\r\n\t]*s[\r\n\t]*c[\r\n\t]*r[\r\n\t]*i[\r\n\t]*p[\r\n\t]*t[\r\n\t]*:/i;
        function il(e) {
          return Sp.test("" + e) ? "javascript:throw new Error('React has blocked a javascript: URL as a security precaution.')" : e;
        }
        var Xo = null;
        function Zo(e) {
          return e = e.target || e.srcElement || window, e.correspondingUseElement && (e = e.correspondingUseElement), e.nodeType === 3 ? e.parentNode : e;
        }
        var Ki = null, Pi = null;
        function yf(e) {
          var t = Vi(e);
          if (t && (e = t.stateNode)) {
            var n = e[zt] || null;
            e: switch (e = t.stateNode, t.type) {
              case "input":
                if (Vo(e, n.value, n.defaultValue, n.defaultValue, n.checked, n.defaultChecked, n.type, n.name), t = n.name, n.type === "radio" && t != null) {
                  for (n = e; n.parentNode; ) n = n.parentNode;
                  for (n = n.querySelectorAll('input[name="' + Kt("" + t) + '"][type="radio"]'), t = 0; t < n.length; t++) {
                    var r = n[t];
                    if (r !== e && r.form === e.form) {
                      var s = r[zt] || null;
                      if (!s) throw Error(i(90));
                      Vo(r, s.value, s.defaultValue, s.defaultValue, s.checked, s.defaultChecked, s.type, s.name);
                    }
                  }
                  for (t = 0; t < n.length; t++) r = n[t], r.form === e.form && df(r);
                }
                break e;
              case "textarea":
                gf(e, n.value, n.defaultValue);
                break e;
              case "select":
                t = n.value, t != null && Zi(e, !!n.multiple, t, false);
            }
          }
        }
        var Qo = false;
        function bf(e, t, n) {
          if (Qo) return e(t, n);
          Qo = true;
          try {
            var r = e(t);
            return r;
          } finally {
            if (Qo = false, (Ki !== null || Pi !== null) && (ql(), Ki && (t = Ki, e = Pi, Pi = Ki = null, yf(t), e))) for (t = 0; t < e.length; t++) yf(e[t]);
          }
        }
        function Va(e, t) {
          var n = e.stateNode;
          if (n === null) return null;
          var r = n[zt] || null;
          if (r === null) return null;
          n = r[t];
          e: switch (t) {
            case "onClick":
            case "onClickCapture":
            case "onDoubleClick":
            case "onDoubleClickCapture":
            case "onMouseDown":
            case "onMouseDownCapture":
            case "onMouseMove":
            case "onMouseMoveCapture":
            case "onMouseUp":
            case "onMouseUpCapture":
            case "onMouseEnter":
              (r = !r.disabled) || (e = e.type, r = !(e === "button" || e === "input" || e === "select" || e === "textarea")), e = !r;
              break e;
            default:
              e = false;
          }
          if (e) return null;
          if (n && typeof n != "function") throw Error(i(231, t, typeof n));
          return n;
        }
        var Rn = !(typeof window > "u" || typeof window.document > "u" || typeof window.document.createElement > "u"), Ko = false;
        if (Rn) try {
          var Ya = {};
          Object.defineProperty(Ya, "passive", {
            get: function() {
              Ko = true;
            }
          }), window.addEventListener("test", Ya, Ya), window.removeEventListener("test", Ya, Ya);
        } catch {
          Ko = false;
        }
        var Kn = null, Po = null, al = null;
        function _f() {
          if (al) return al;
          var e, t = Po, n = t.length, r, s = "value" in Kn ? Kn.value : Kn.textContent, f = s.length;
          for (e = 0; e < n && t[e] === s[e]; e++) ;
          var v = n - e;
          for (r = 1; r <= v && t[n - r] === s[f - r]; r++) ;
          return al = s.slice(e, 1 < r ? 1 - r : void 0);
        }
        function rl(e) {
          var t = e.keyCode;
          return "charCode" in e ? (e = e.charCode, e === 0 && t === 13 && (e = 13)) : e = t, e === 10 && (e = 13), 32 <= e || e === 13 ? e : 0;
        }
        function ll() {
          return true;
        }
        function wf() {
          return false;
        }
        function kt(e) {
          function t(n, r, s, f, v) {
            this._reactName = n, this._targetInst = s, this.type = r, this.nativeEvent = f, this.target = v, this.currentTarget = null;
            for (var b in e) e.hasOwnProperty(b) && (n = e[b], this[b] = n ? n(f) : f[b]);
            return this.isDefaultPrevented = (f.defaultPrevented != null ? f.defaultPrevented : f.returnValue === false) ? ll : wf, this.isPropagationStopped = wf, this;
          }
          return p(t.prototype, {
            preventDefault: function() {
              this.defaultPrevented = true;
              var n = this.nativeEvent;
              n && (n.preventDefault ? n.preventDefault() : typeof n.returnValue != "unknown" && (n.returnValue = false), this.isDefaultPrevented = ll);
            },
            stopPropagation: function() {
              var n = this.nativeEvent;
              n && (n.stopPropagation ? n.stopPropagation() : typeof n.cancelBubble != "unknown" && (n.cancelBubble = true), this.isPropagationStopped = ll);
            },
            persist: function() {
            },
            isPersistent: ll
          }), t;
        }
        var Ei = {
          eventPhase: 0,
          bubbles: 0,
          cancelable: 0,
          timeStamp: function(e) {
            return e.timeStamp || Date.now();
          },
          defaultPrevented: 0,
          isTrusted: 0
        }, ol = kt(Ei), $a = p({}, Ei, {
          view: 0,
          detail: 0
        }), xp = kt($a), Io, Wo, Xa, ul = p({}, $a, {
          screenX: 0,
          screenY: 0,
          clientX: 0,
          clientY: 0,
          pageX: 0,
          pageY: 0,
          ctrlKey: 0,
          shiftKey: 0,
          altKey: 0,
          metaKey: 0,
          getModifierState: eu,
          button: 0,
          buttons: 0,
          relatedTarget: function(e) {
            return e.relatedTarget === void 0 ? e.fromElement === e.srcElement ? e.toElement : e.fromElement : e.relatedTarget;
          },
          movementX: function(e) {
            return "movementX" in e ? e.movementX : (e !== Xa && (Xa && e.type === "mousemove" ? (Io = e.screenX - Xa.screenX, Wo = e.screenY - Xa.screenY) : Wo = Io = 0, Xa = e), Io);
          },
          movementY: function(e) {
            return "movementY" in e ? e.movementY : Wo;
          }
        }), Ef = kt(ul), Tp = p({}, ul, {
          dataTransfer: 0
        }), Ap = kt(Tp), Rp = p({}, $a, {
          relatedTarget: 0
        }), Jo = kt(Rp), Cp = p({}, Ei, {
          animationName: 0,
          elapsedTime: 0,
          pseudoElement: 0
        }), Dp = kt(Cp), Np = p({}, Ei, {
          clipboardData: function(e) {
            return "clipboardData" in e ? e.clipboardData : window.clipboardData;
          }
        }), Op = kt(Np), zp = p({}, Ei, {
          data: 0
        }), Sf = kt(zp), kp = {
          Esc: "Escape",
          Spacebar: " ",
          Left: "ArrowLeft",
          Up: "ArrowUp",
          Right: "ArrowRight",
          Down: "ArrowDown",
          Del: "Delete",
          Win: "OS",
          Menu: "ContextMenu",
          Apps: "ContextMenu",
          Scroll: "ScrollLock",
          MozPrintableKey: "Unidentified"
        }, Mp = {
          8: "Backspace",
          9: "Tab",
          12: "Clear",
          13: "Enter",
          16: "Shift",
          17: "Control",
          18: "Alt",
          19: "Pause",
          20: "CapsLock",
          27: "Escape",
          32: " ",
          33: "PageUp",
          34: "PageDown",
          35: "End",
          36: "Home",
          37: "ArrowLeft",
          38: "ArrowUp",
          39: "ArrowRight",
          40: "ArrowDown",
          45: "Insert",
          46: "Delete",
          112: "F1",
          113: "F2",
          114: "F3",
          115: "F4",
          116: "F5",
          117: "F6",
          118: "F7",
          119: "F8",
          120: "F9",
          121: "F10",
          122: "F11",
          123: "F12",
          144: "NumLock",
          145: "ScrollLock",
          224: "Meta"
        }, Lp = {
          Alt: "altKey",
          Control: "ctrlKey",
          Meta: "metaKey",
          Shift: "shiftKey"
        };
        function Gp(e) {
          var t = this.nativeEvent;
          return t.getModifierState ? t.getModifierState(e) : (e = Lp[e]) ? !!t[e] : false;
        }
        function eu() {
          return Gp;
        }
        var Up = p({}, $a, {
          key: function(e) {
            if (e.key) {
              var t = kp[e.key] || e.key;
              if (t !== "Unidentified") return t;
            }
            return e.type === "keypress" ? (e = rl(e), e === 13 ? "Enter" : String.fromCharCode(e)) : e.type === "keydown" || e.type === "keyup" ? Mp[e.keyCode] || "Unidentified" : "";
          },
          code: 0,
          location: 0,
          ctrlKey: 0,
          shiftKey: 0,
          altKey: 0,
          metaKey: 0,
          repeat: 0,
          locale: 0,
          getModifierState: eu,
          charCode: function(e) {
            return e.type === "keypress" ? rl(e) : 0;
          },
          keyCode: function(e) {
            return e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
          },
          which: function(e) {
            return e.type === "keypress" ? rl(e) : e.type === "keydown" || e.type === "keyup" ? e.keyCode : 0;
          }
        }), jp = kt(Up), Bp = p({}, ul, {
          pointerId: 0,
          width: 0,
          height: 0,
          pressure: 0,
          tangentialPressure: 0,
          tiltX: 0,
          tiltY: 0,
          twist: 0,
          pointerType: 0,
          isPrimary: 0
        }), xf = kt(Bp), Hp = p({}, $a, {
          touches: 0,
          targetTouches: 0,
          changedTouches: 0,
          altKey: 0,
          metaKey: 0,
          ctrlKey: 0,
          shiftKey: 0,
          getModifierState: eu
        }), Fp = kt(Hp), qp = p({}, Ei, {
          propertyName: 0,
          elapsedTime: 0,
          pseudoElement: 0
        }), Vp = kt(qp), Yp = p({}, ul, {
          deltaX: function(e) {
            return "deltaX" in e ? e.deltaX : "wheelDeltaX" in e ? -e.wheelDeltaX : 0;
          },
          deltaY: function(e) {
            return "deltaY" in e ? e.deltaY : "wheelDeltaY" in e ? -e.wheelDeltaY : "wheelDelta" in e ? -e.wheelDelta : 0;
          },
          deltaZ: 0,
          deltaMode: 0
        }), $p = kt(Yp), Xp = p({}, Ei, {
          newState: 0,
          oldState: 0
        }), Zp = kt(Xp), Qp = [
          9,
          13,
          27,
          32
        ], tu = Rn && "CompositionEvent" in window, Za = null;
        Rn && "documentMode" in document && (Za = document.documentMode);
        var Kp = Rn && "TextEvent" in window && !Za, Tf = Rn && (!tu || Za && 8 < Za && 11 >= Za), Af = " ", Rf = false;
        function Cf(e, t) {
          switch (e) {
            case "keyup":
              return Qp.indexOf(t.keyCode) !== -1;
            case "keydown":
              return t.keyCode !== 229;
            case "keypress":
            case "mousedown":
            case "focusout":
              return true;
            default:
              return false;
          }
        }
        function Df(e) {
          return e = e.detail, typeof e == "object" && "data" in e ? e.data : null;
        }
        var Ii = false;
        function Pp(e, t) {
          switch (e) {
            case "compositionend":
              return Df(t);
            case "keypress":
              return t.which !== 32 ? null : (Rf = true, Af);
            case "textInput":
              return e = t.data, e === Af && Rf ? null : e;
            default:
              return null;
          }
        }
        function Ip(e, t) {
          if (Ii) return e === "compositionend" || !tu && Cf(e, t) ? (e = _f(), al = Po = Kn = null, Ii = false, e) : null;
          switch (e) {
            case "paste":
              return null;
            case "keypress":
              if (!(t.ctrlKey || t.altKey || t.metaKey) || t.ctrlKey && t.altKey) {
                if (t.char && 1 < t.char.length) return t.char;
                if (t.which) return String.fromCharCode(t.which);
              }
              return null;
            case "compositionend":
              return Tf && t.locale !== "ko" ? null : t.data;
            default:
              return null;
          }
        }
        var Wp = {
          color: true,
          date: true,
          datetime: true,
          "datetime-local": true,
          email: true,
          month: true,
          number: true,
          password: true,
          range: true,
          search: true,
          tel: true,
          text: true,
          time: true,
          url: true,
          week: true
        };
        function Nf(e) {
          var t = e && e.nodeName && e.nodeName.toLowerCase();
          return t === "input" ? !!Wp[e.type] : t === "textarea";
        }
        function Of(e, t, n, r) {
          Ki ? Pi ? Pi.push(r) : Pi = [
            r
          ] : Ki = r, t = Ql(t, "onChange"), 0 < t.length && (n = new ol("onChange", "change", null, n, r), e.push({
            event: n,
            listeners: t
          }));
        }
        var Qa = null, Ka = null;
        function Jp(e) {
          fg(e, 0);
        }
        function sl(e) {
          var t = qa(e);
          if (df(t)) return e;
        }
        function zf(e, t) {
          if (e === "change") return t;
        }
        var kf = false;
        if (Rn) {
          var nu;
          if (Rn) {
            var iu = "oninput" in document;
            if (!iu) {
              var Mf = document.createElement("div");
              Mf.setAttribute("oninput", "return;"), iu = typeof Mf.oninput == "function";
            }
            nu = iu;
          } else nu = false;
          kf = nu && (!document.documentMode || 9 < document.documentMode);
        }
        function Lf() {
          Qa && (Qa.detachEvent("onpropertychange", Gf), Ka = Qa = null);
        }
        function Gf(e) {
          if (e.propertyName === "value" && sl(Ka)) {
            var t = [];
            Of(t, Ka, e, Zo(e)), bf(Jp, t);
          }
        }
        function ey(e, t, n) {
          e === "focusin" ? (Lf(), Qa = t, Ka = n, Qa.attachEvent("onpropertychange", Gf)) : e === "focusout" && Lf();
        }
        function ty(e) {
          if (e === "selectionchange" || e === "keyup" || e === "keydown") return sl(Ka);
        }
        function ny(e, t) {
          if (e === "click") return sl(t);
        }
        function iy(e, t) {
          if (e === "input" || e === "change") return sl(t);
        }
        function ay(e, t) {
          return e === t && (e !== 0 || 1 / e === 1 / t) || e !== e && t !== t;
        }
        var Ut = typeof Object.is == "function" ? Object.is : ay;
        function Pa(e, t) {
          if (Ut(e, t)) return true;
          if (typeof e != "object" || e === null || typeof t != "object" || t === null) return false;
          var n = Object.keys(e), r = Object.keys(t);
          if (n.length !== r.length) return false;
          for (r = 0; r < n.length; r++) {
            var s = n[r];
            if (!Me.call(t, s) || !Ut(e[s], t[s])) return false;
          }
          return true;
        }
        function Uf(e) {
          for (; e && e.firstChild; ) e = e.firstChild;
          return e;
        }
        function jf(e, t) {
          var n = Uf(e);
          e = 0;
          for (var r; n; ) {
            if (n.nodeType === 3) {
              if (r = e + n.textContent.length, e <= t && r >= t) return {
                node: n,
                offset: t - e
              };
              e = r;
            }
            e: {
              for (; n; ) {
                if (n.nextSibling) {
                  n = n.nextSibling;
                  break e;
                }
                n = n.parentNode;
              }
              n = void 0;
            }
            n = Uf(n);
          }
        }
        function Bf(e, t) {
          return e && t ? e === t ? true : e && e.nodeType === 3 ? false : t && t.nodeType === 3 ? Bf(e, t.parentNode) : "contains" in e ? e.contains(t) : e.compareDocumentPosition ? !!(e.compareDocumentPosition(t) & 16) : false : false;
        }
        function Hf(e) {
          e = e != null && e.ownerDocument != null && e.ownerDocument.defaultView != null ? e.ownerDocument.defaultView : window;
          for (var t = nl(e.document); t instanceof e.HTMLIFrameElement; ) {
            try {
              var n = typeof t.contentWindow.location.href == "string";
            } catch {
              n = false;
            }
            if (n) e = t.contentWindow;
            else break;
            t = nl(e.document);
          }
          return t;
        }
        function au(e) {
          var t = e && e.nodeName && e.nodeName.toLowerCase();
          return t && (t === "input" && (e.type === "text" || e.type === "search" || e.type === "tel" || e.type === "url" || e.type === "password") || t === "textarea" || e.contentEditable === "true");
        }
        var ry = Rn && "documentMode" in document && 11 >= document.documentMode, Wi = null, ru = null, Ia = null, lu = false;
        function Ff(e, t, n) {
          var r = n.window === n ? n.document : n.nodeType === 9 ? n : n.ownerDocument;
          lu || Wi == null || Wi !== nl(r) || (r = Wi, "selectionStart" in r && au(r) ? r = {
            start: r.selectionStart,
            end: r.selectionEnd
          } : (r = (r.ownerDocument && r.ownerDocument.defaultView || window).getSelection(), r = {
            anchorNode: r.anchorNode,
            anchorOffset: r.anchorOffset,
            focusNode: r.focusNode,
            focusOffset: r.focusOffset
          }), Ia && Pa(Ia, r) || (Ia = r, r = Ql(ru, "onSelect"), 0 < r.length && (t = new ol("onSelect", "select", null, t, n), e.push({
            event: t,
            listeners: r
          }), t.target = Wi)));
        }
        function Si(e, t) {
          var n = {};
          return n[e.toLowerCase()] = t.toLowerCase(), n["Webkit" + e] = "webkit" + t, n["Moz" + e] = "moz" + t, n;
        }
        var Ji = {
          animationend: Si("Animation", "AnimationEnd"),
          animationiteration: Si("Animation", "AnimationIteration"),
          animationstart: Si("Animation", "AnimationStart"),
          transitionrun: Si("Transition", "TransitionRun"),
          transitionstart: Si("Transition", "TransitionStart"),
          transitioncancel: Si("Transition", "TransitionCancel"),
          transitionend: Si("Transition", "TransitionEnd")
        }, ou = {}, qf = {};
        Rn && (qf = document.createElement("div").style, "AnimationEvent" in window || (delete Ji.animationend.animation, delete Ji.animationiteration.animation, delete Ji.animationstart.animation), "TransitionEvent" in window || delete Ji.transitionend.transition);
        function xi(e) {
          if (ou[e]) return ou[e];
          if (!Ji[e]) return e;
          var t = Ji[e], n;
          for (n in t) if (t.hasOwnProperty(n) && n in qf) return ou[e] = t[n];
          return e;
        }
        var Vf = xi("animationend"), Yf = xi("animationiteration"), $f = xi("animationstart"), ly = xi("transitionrun"), oy = xi("transitionstart"), uy = xi("transitioncancel"), Xf = xi("transitionend"), Zf = /* @__PURE__ */ new Map(), uu = "abort auxClick beforeToggle cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");
        uu.push("scrollEnd");
        function cn(e, t) {
          Zf.set(e, t), wi(t, [
            e
          ]);
        }
        var Qf = /* @__PURE__ */ new WeakMap();
        function Pt(e, t) {
          if (typeof e == "object" && e !== null) {
            var n = Qf.get(e);
            return n !== void 0 ? n : (t = {
              value: e,
              source: t,
              stack: cf(t)
            }, Qf.set(e, t), t);
          }
          return {
            value: e,
            source: t,
            stack: cf(t)
          };
        }
        var It = [], ea = 0, su = 0;
        function cl() {
          for (var e = ea, t = su = ea = 0; t < e; ) {
            var n = It[t];
            It[t++] = null;
            var r = It[t];
            It[t++] = null;
            var s = It[t];
            It[t++] = null;
            var f = It[t];
            if (It[t++] = null, r !== null && s !== null) {
              var v = r.pending;
              v === null ? s.next = s : (s.next = v.next, v.next = s), r.pending = s;
            }
            f !== 0 && Kf(n, s, f);
          }
        }
        function fl(e, t, n, r) {
          It[ea++] = e, It[ea++] = t, It[ea++] = n, It[ea++] = r, su |= r, e.lanes |= r, e = e.alternate, e !== null && (e.lanes |= r);
        }
        function cu(e, t, n, r) {
          return fl(e, t, n, r), dl(e);
        }
        function ta(e, t) {
          return fl(e, null, null, t), dl(e);
        }
        function Kf(e, t, n) {
          e.lanes |= n;
          var r = e.alternate;
          r !== null && (r.lanes |= n);
          for (var s = false, f = e.return; f !== null; ) f.childLanes |= n, r = f.alternate, r !== null && (r.childLanes |= n), f.tag === 22 && (e = f.stateNode, e === null || e._visibility & 1 || (s = true)), e = f, f = f.return;
          return e.tag === 3 ? (f = e.stateNode, s && t !== null && (s = 31 - wt(n), e = f.hiddenUpdates, r = e[s], r === null ? e[s] = [
            t
          ] : r.push(t), t.lane = n | 536870912), f) : null;
        }
        function dl(e) {
          if (50 < Er) throw Er = 0, vs = null, Error(i(185));
          for (var t = e.return; t !== null; ) e = t, t = e.return;
          return e.tag === 3 ? e.stateNode : null;
        }
        var na = {};
        function sy(e, t, n, r) {
          this.tag = e, this.key = n, this.sibling = this.child = this.return = this.stateNode = this.type = this.elementType = null, this.index = 0, this.refCleanup = this.ref = null, this.pendingProps = t, this.dependencies = this.memoizedState = this.updateQueue = this.memoizedProps = null, this.mode = r, this.subtreeFlags = this.flags = 0, this.deletions = null, this.childLanes = this.lanes = 0, this.alternate = null;
        }
        function jt(e, t, n, r) {
          return new sy(e, t, n, r);
        }
        function fu(e) {
          return e = e.prototype, !(!e || !e.isReactComponent);
        }
        function Cn(e, t) {
          var n = e.alternate;
          return n === null ? (n = jt(e.tag, t, e.key, e.mode), n.elementType = e.elementType, n.type = e.type, n.stateNode = e.stateNode, n.alternate = e, e.alternate = n) : (n.pendingProps = t, n.type = e.type, n.flags = 0, n.subtreeFlags = 0, n.deletions = null), n.flags = e.flags & 65011712, n.childLanes = e.childLanes, n.lanes = e.lanes, n.child = e.child, n.memoizedProps = e.memoizedProps, n.memoizedState = e.memoizedState, n.updateQueue = e.updateQueue, t = e.dependencies, n.dependencies = t === null ? null : {
            lanes: t.lanes,
            firstContext: t.firstContext
          }, n.sibling = e.sibling, n.index = e.index, n.ref = e.ref, n.refCleanup = e.refCleanup, n;
        }
        function Pf(e, t) {
          e.flags &= 65011714;
          var n = e.alternate;
          return n === null ? (e.childLanes = 0, e.lanes = t, e.child = null, e.subtreeFlags = 0, e.memoizedProps = null, e.memoizedState = null, e.updateQueue = null, e.dependencies = null, e.stateNode = null) : (e.childLanes = n.childLanes, e.lanes = n.lanes, e.child = n.child, e.subtreeFlags = 0, e.deletions = null, e.memoizedProps = n.memoizedProps, e.memoizedState = n.memoizedState, e.updateQueue = n.updateQueue, e.type = n.type, t = n.dependencies, e.dependencies = t === null ? null : {
            lanes: t.lanes,
            firstContext: t.firstContext
          }), e;
        }
        function hl(e, t, n, r, s, f) {
          var v = 0;
          if (r = e, typeof e == "function") fu(e) && (v = 1);
          else if (typeof e == "string") v = fb(e, n, ie.current) ? 26 : e === "html" || e === "head" || e === "body" ? 27 : 5;
          else e: switch (e) {
            case w:
              return e = jt(31, n, t, s), e.elementType = w, e.lanes = f, e;
            case z:
              return Ti(n.children, s, f, t);
            case H:
              v = 8, s |= 24;
              break;
            case Y:
              return e = jt(12, n, t, s | 2), e.elementType = Y, e.lanes = f, e;
            case B:
              return e = jt(13, n, t, s), e.elementType = B, e.lanes = f, e;
            case Z:
              return e = jt(19, n, t, s), e.elementType = Z, e.lanes = f, e;
            default:
              if (typeof e == "object" && e !== null) switch (e.$$typeof) {
                case te:
                case I:
                  v = 10;
                  break e;
                case J:
                  v = 9;
                  break e;
                case G:
                  v = 11;
                  break e;
                case T:
                  v = 14;
                  break e;
                case y:
                  v = 16, r = null;
                  break e;
              }
              v = 29, n = Error(i(130, e === null ? "null" : typeof e, "")), r = null;
          }
          return t = jt(v, n, t, s), t.elementType = e, t.type = r, t.lanes = f, t;
        }
        function Ti(e, t, n, r) {
          return e = jt(7, e, r, t), e.lanes = n, e;
        }
        function du(e, t, n) {
          return e = jt(6, e, null, t), e.lanes = n, e;
        }
        function hu(e, t, n) {
          return t = jt(4, e.children !== null ? e.children : [], e.key, t), t.lanes = n, t.stateNode = {
            containerInfo: e.containerInfo,
            pendingChildren: null,
            implementation: e.implementation
          }, t;
        }
        var ia = [], aa = 0, gl = null, ml = 0, Wt = [], Jt = 0, Ai = null, Dn = 1, Nn = "";
        function Ri(e, t) {
          ia[aa++] = ml, ia[aa++] = gl, gl = e, ml = t;
        }
        function If(e, t, n) {
          Wt[Jt++] = Dn, Wt[Jt++] = Nn, Wt[Jt++] = Ai, Ai = e;
          var r = Dn;
          e = Nn;
          var s = 32 - wt(r) - 1;
          r &= ~(1 << s), n += 1;
          var f = 32 - wt(t) + s;
          if (30 < f) {
            var v = s - s % 5;
            f = (r & (1 << v) - 1).toString(32), r >>= v, s -= v, Dn = 1 << 32 - wt(t) + s | n << s | r, Nn = f + e;
          } else Dn = 1 << f | n << s | r, Nn = e;
        }
        function gu(e) {
          e.return !== null && (Ri(e, 1), If(e, 1, 0));
        }
        function mu(e) {
          for (; e === gl; ) gl = ia[--aa], ia[aa] = null, ml = ia[--aa], ia[aa] = null;
          for (; e === Ai; ) Ai = Wt[--Jt], Wt[Jt] = null, Nn = Wt[--Jt], Wt[Jt] = null, Dn = Wt[--Jt], Wt[Jt] = null;
        }
        var Ot = null, it = null, Be = false, Ci = null, bn = false, vu = Error(i(519));
        function Di(e) {
          var t = Error(i(418, ""));
          throw er(Pt(t, e)), vu;
        }
        function Wf(e) {
          var t = e.stateNode, n = e.type, r = e.memoizedProps;
          switch (t[At] = e, t[zt] = r, n) {
            case "dialog":
              ze("cancel", t), ze("close", t);
              break;
            case "iframe":
            case "object":
            case "embed":
              ze("load", t);
              break;
            case "video":
            case "audio":
              for (n = 0; n < xr.length; n++) ze(xr[n], t);
              break;
            case "source":
              ze("error", t);
              break;
            case "img":
            case "image":
            case "link":
              ze("error", t), ze("load", t);
              break;
            case "details":
              ze("toggle", t);
              break;
            case "input":
              ze("invalid", t), hf(t, r.value, r.defaultValue, r.checked, r.defaultChecked, r.type, r.name, true), tl(t);
              break;
            case "select":
              ze("invalid", t);
              break;
            case "textarea":
              ze("invalid", t), mf(t, r.value, r.defaultValue, r.children), tl(t);
          }
          n = r.children, typeof n != "string" && typeof n != "number" && typeof n != "bigint" || t.textContent === "" + n || r.suppressHydrationWarning === true || mg(t.textContent, n) ? (r.popover != null && (ze("beforetoggle", t), ze("toggle", t)), r.onScroll != null && ze("scroll", t), r.onScrollEnd != null && ze("scrollend", t), r.onClick != null && (t.onclick = Kl), t = true) : t = false, t || Di(e);
        }
        function Jf(e) {
          for (Ot = e.return; Ot; ) switch (Ot.tag) {
            case 5:
            case 13:
              bn = false;
              return;
            case 27:
            case 3:
              bn = true;
              return;
            default:
              Ot = Ot.return;
          }
        }
        function Wa(e) {
          if (e !== Ot) return false;
          if (!Be) return Jf(e), Be = true, false;
          var t = e.tag, n;
          if ((n = t !== 3 && t !== 27) && ((n = t === 5) && (n = e.type, n = !(n !== "form" && n !== "button") || zs(e.type, e.memoizedProps)), n = !n), n && it && Di(e), Jf(e), t === 13) {
            if (e = e.memoizedState, e = e !== null ? e.dehydrated : null, !e) throw Error(i(317));
            e: {
              for (e = e.nextSibling, t = 0; e; ) {
                if (e.nodeType === 8) if (n = e.data, n === "/$") {
                  if (t === 0) {
                    it = dn(e.nextSibling);
                    break e;
                  }
                  t--;
                } else n !== "$" && n !== "$!" && n !== "$?" || t++;
                e = e.nextSibling;
              }
              it = null;
            }
          } else t === 27 ? (t = it, fi(e.type) ? (e = Gs, Gs = null, it = e) : it = t) : it = Ot ? dn(e.stateNode.nextSibling) : null;
          return true;
        }
        function Ja() {
          it = Ot = null, Be = false;
        }
        function ed() {
          var e = Ci;
          return e !== null && (Gt === null ? Gt = e : Gt.push.apply(Gt, e), Ci = null), e;
        }
        function er(e) {
          Ci === null ? Ci = [
            e
          ] : Ci.push(e);
        }
        var pu = k(null), Ni = null, On = null;
        function Pn(e, t, n) {
          U(pu, t._currentValue), t._currentValue = n;
        }
        function zn(e) {
          e._currentValue = pu.current, K(pu);
        }
        function yu(e, t, n) {
          for (; e !== null; ) {
            var r = e.alternate;
            if ((e.childLanes & t) !== t ? (e.childLanes |= t, r !== null && (r.childLanes |= t)) : r !== null && (r.childLanes & t) !== t && (r.childLanes |= t), e === n) break;
            e = e.return;
          }
        }
        function bu(e, t, n, r) {
          var s = e.child;
          for (s !== null && (s.return = e); s !== null; ) {
            var f = s.dependencies;
            if (f !== null) {
              var v = s.child;
              f = f.firstContext;
              e: for (; f !== null; ) {
                var b = f;
                f = s;
                for (var A = 0; A < t.length; A++) if (b.context === t[A]) {
                  f.lanes |= n, b = f.alternate, b !== null && (b.lanes |= n), yu(f.return, n, e), r || (v = null);
                  break e;
                }
                f = b.next;
              }
            } else if (s.tag === 18) {
              if (v = s.return, v === null) throw Error(i(341));
              v.lanes |= n, f = v.alternate, f !== null && (f.lanes |= n), yu(v, n, e), v = null;
            } else v = s.child;
            if (v !== null) v.return = s;
            else for (v = s; v !== null; ) {
              if (v === e) {
                v = null;
                break;
              }
              if (s = v.sibling, s !== null) {
                s.return = v.return, v = s;
                break;
              }
              v = v.return;
            }
            s = v;
          }
        }
        function tr(e, t, n, r) {
          e = null;
          for (var s = t, f = false; s !== null; ) {
            if (!f) {
              if ((s.flags & 524288) !== 0) f = true;
              else if ((s.flags & 262144) !== 0) break;
            }
            if (s.tag === 10) {
              var v = s.alternate;
              if (v === null) throw Error(i(387));
              if (v = v.memoizedProps, v !== null) {
                var b = s.type;
                Ut(s.pendingProps.value, v.value) || (e !== null ? e.push(b) : e = [
                  b
                ]);
              }
            } else if (s === be.current) {
              if (v = s.alternate, v === null) throw Error(i(387));
              v.memoizedState.memoizedState !== s.memoizedState.memoizedState && (e !== null ? e.push(Nr) : e = [
                Nr
              ]);
            }
            s = s.return;
          }
          e !== null && bu(t, e, n, r), t.flags |= 262144;
        }
        function vl(e) {
          for (e = e.firstContext; e !== null; ) {
            if (!Ut(e.context._currentValue, e.memoizedValue)) return true;
            e = e.next;
          }
          return false;
        }
        function Oi(e) {
          Ni = e, On = null, e = e.dependencies, e !== null && (e.firstContext = null);
        }
        function Rt(e) {
          return td(Ni, e);
        }
        function pl(e, t) {
          return Ni === null && Oi(e), td(e, t);
        }
        function td(e, t) {
          var n = t._currentValue;
          if (t = {
            context: t,
            memoizedValue: n,
            next: null
          }, On === null) {
            if (e === null) throw Error(i(308));
            On = t, e.dependencies = {
              lanes: 0,
              firstContext: t
            }, e.flags |= 524288;
          } else On = On.next = t;
          return n;
        }
        var cy = typeof AbortController < "u" ? AbortController : function() {
          var e = [], t = this.signal = {
            aborted: false,
            addEventListener: function(n, r) {
              e.push(r);
            }
          };
          this.abort = function() {
            t.aborted = true, e.forEach(function(n) {
              return n();
            });
          };
        }, fy = l.unstable_scheduleCallback, dy = l.unstable_NormalPriority, ct = {
          $$typeof: I,
          Consumer: null,
          Provider: null,
          _currentValue: null,
          _currentValue2: null,
          _threadCount: 0
        };
        function _u() {
          return {
            controller: new cy(),
            data: /* @__PURE__ */ new Map(),
            refCount: 0
          };
        }
        function nr(e) {
          e.refCount--, e.refCount === 0 && fy(dy, function() {
            e.controller.abort();
          });
        }
        var ir = null, wu = 0, ra = 0, la = null;
        function hy(e, t) {
          if (ir === null) {
            var n = ir = [];
            wu = 0, ra = Ss(), la = {
              status: "pending",
              value: void 0,
              then: function(r) {
                n.push(r);
              }
            };
          }
          return wu++, t.then(nd, nd), t;
        }
        function nd() {
          if (--wu === 0 && ir !== null) {
            la !== null && (la.status = "fulfilled");
            var e = ir;
            ir = null, ra = 0, la = null;
            for (var t = 0; t < e.length; t++) (0, e[t])();
          }
        }
        function gy(e, t) {
          var n = [], r = {
            status: "pending",
            value: null,
            reason: null,
            then: function(s) {
              n.push(s);
            }
          };
          return e.then(function() {
            r.status = "fulfilled", r.value = t;
            for (var s = 0; s < n.length; s++) (0, n[s])(t);
          }, function(s) {
            for (r.status = "rejected", r.reason = s, s = 0; s < n.length; s++) (0, n[s])(void 0);
          }), r;
        }
        var id = D.S;
        D.S = function(e, t) {
          typeof t == "object" && t !== null && typeof t.then == "function" && hy(e, t), id !== null && id(e, t);
        };
        var zi = k(null);
        function Eu() {
          var e = zi.current;
          return e !== null ? e : Pe.pooledCache;
        }
        function yl(e, t) {
          t === null ? U(zi, zi.current) : U(zi, t.pool);
        }
        function ad() {
          var e = Eu();
          return e === null ? null : {
            parent: ct._currentValue,
            pool: e
          };
        }
        var ar = Error(i(460)), rd = Error(i(474)), bl = Error(i(542)), Su = {
          then: function() {
          }
        };
        function ld(e) {
          return e = e.status, e === "fulfilled" || e === "rejected";
        }
        function _l() {
        }
        function od(e, t, n) {
          switch (n = e[n], n === void 0 ? e.push(t) : n !== t && (t.then(_l, _l), t = n), t.status) {
            case "fulfilled":
              return t.value;
            case "rejected":
              throw e = t.reason, sd(e), e;
            default:
              if (typeof t.status == "string") t.then(_l, _l);
              else {
                if (e = Pe, e !== null && 100 < e.shellSuspendCounter) throw Error(i(482));
                e = t, e.status = "pending", e.then(function(r) {
                  if (t.status === "pending") {
                    var s = t;
                    s.status = "fulfilled", s.value = r;
                  }
                }, function(r) {
                  if (t.status === "pending") {
                    var s = t;
                    s.status = "rejected", s.reason = r;
                  }
                });
              }
              switch (t.status) {
                case "fulfilled":
                  return t.value;
                case "rejected":
                  throw e = t.reason, sd(e), e;
              }
              throw rr = t, ar;
          }
        }
        var rr = null;
        function ud() {
          if (rr === null) throw Error(i(459));
          var e = rr;
          return rr = null, e;
        }
        function sd(e) {
          if (e === ar || e === bl) throw Error(i(483));
        }
        var In = false;
        function xu(e) {
          e.updateQueue = {
            baseState: e.memoizedState,
            firstBaseUpdate: null,
            lastBaseUpdate: null,
            shared: {
              pending: null,
              lanes: 0,
              hiddenCallbacks: null
            },
            callbacks: null
          };
        }
        function Tu(e, t) {
          e = e.updateQueue, t.updateQueue === e && (t.updateQueue = {
            baseState: e.baseState,
            firstBaseUpdate: e.firstBaseUpdate,
            lastBaseUpdate: e.lastBaseUpdate,
            shared: e.shared,
            callbacks: null
          });
        }
        function Wn(e) {
          return {
            lane: e,
            tag: 0,
            payload: null,
            callback: null,
            next: null
          };
        }
        function Jn(e, t, n) {
          var r = e.updateQueue;
          if (r === null) return null;
          if (r = r.shared, (He & 2) !== 0) {
            var s = r.pending;
            return s === null ? t.next = t : (t.next = s.next, s.next = t), r.pending = t, t = dl(e), Kf(e, null, n), t;
          }
          return fl(e, r, t, n), dl(e);
        }
        function lr(e, t, n) {
          if (t = t.updateQueue, t !== null && (t = t.shared, (n & 4194048) !== 0)) {
            var r = t.lanes;
            r &= e.pendingLanes, n |= r, t.lanes = n, tf(e, n);
          }
        }
        function Au(e, t) {
          var n = e.updateQueue, r = e.alternate;
          if (r !== null && (r = r.updateQueue, n === r)) {
            var s = null, f = null;
            if (n = n.firstBaseUpdate, n !== null) {
              do {
                var v = {
                  lane: n.lane,
                  tag: n.tag,
                  payload: n.payload,
                  callback: null,
                  next: null
                };
                f === null ? s = f = v : f = f.next = v, n = n.next;
              } while (n !== null);
              f === null ? s = f = t : f = f.next = t;
            } else s = f = t;
            n = {
              baseState: r.baseState,
              firstBaseUpdate: s,
              lastBaseUpdate: f,
              shared: r.shared,
              callbacks: r.callbacks
            }, e.updateQueue = n;
            return;
          }
          e = n.lastBaseUpdate, e === null ? n.firstBaseUpdate = t : e.next = t, n.lastBaseUpdate = t;
        }
        var Ru = false;
        function or() {
          if (Ru) {
            var e = la;
            if (e !== null) throw e;
          }
        }
        function ur(e, t, n, r) {
          Ru = false;
          var s = e.updateQueue;
          In = false;
          var f = s.firstBaseUpdate, v = s.lastBaseUpdate, b = s.shared.pending;
          if (b !== null) {
            s.shared.pending = null;
            var A = b, j = A.next;
            A.next = null, v === null ? f = j : v.next = j, v = A;
            var Q = e.alternate;
            Q !== null && (Q = Q.updateQueue, b = Q.lastBaseUpdate, b !== v && (b === null ? Q.firstBaseUpdate = j : b.next = j, Q.lastBaseUpdate = A));
          }
          if (f !== null) {
            var W = s.baseState;
            v = 0, Q = j = A = null, b = f;
            do {
              var F = b.lane & -536870913, q = F !== b.lane;
              if (q ? (Ge & F) === F : (r & F) === F) {
                F !== 0 && F === ra && (Ru = true), Q !== null && (Q = Q.next = {
                  lane: 0,
                  tag: b.tag,
                  payload: b.payload,
                  callback: null,
                  next: null
                });
                e: {
                  var xe = e, we = b;
                  F = t;
                  var Xe = n;
                  switch (we.tag) {
                    case 1:
                      if (xe = we.payload, typeof xe == "function") {
                        W = xe.call(Xe, W, F);
                        break e;
                      }
                      W = xe;
                      break e;
                    case 3:
                      xe.flags = xe.flags & -65537 | 128;
                    case 0:
                      if (xe = we.payload, F = typeof xe == "function" ? xe.call(Xe, W, F) : xe, F == null) break e;
                      W = p({}, W, F);
                      break e;
                    case 2:
                      In = true;
                  }
                }
                F = b.callback, F !== null && (e.flags |= 64, q && (e.flags |= 8192), q = s.callbacks, q === null ? s.callbacks = [
                  F
                ] : q.push(F));
              } else q = {
                lane: F,
                tag: b.tag,
                payload: b.payload,
                callback: b.callback,
                next: null
              }, Q === null ? (j = Q = q, A = W) : Q = Q.next = q, v |= F;
              if (b = b.next, b === null) {
                if (b = s.shared.pending, b === null) break;
                q = b, b = q.next, q.next = null, s.lastBaseUpdate = q, s.shared.pending = null;
              }
            } while (true);
            Q === null && (A = W), s.baseState = A, s.firstBaseUpdate = j, s.lastBaseUpdate = Q, f === null && (s.shared.lanes = 0), oi |= v, e.lanes = v, e.memoizedState = W;
          }
        }
        function cd(e, t) {
          if (typeof e != "function") throw Error(i(191, e));
          e.call(t);
        }
        function fd(e, t) {
          var n = e.callbacks;
          if (n !== null) for (e.callbacks = null, e = 0; e < n.length; e++) cd(n[e], t);
        }
        var oa = k(null), wl = k(0);
        function dd(e, t) {
          e = Bn, U(wl, e), U(oa, t), Bn = e | t.baseLanes;
        }
        function Cu() {
          U(wl, Bn), U(oa, oa.current);
        }
        function Du() {
          Bn = wl.current, K(oa), K(wl);
        }
        var ei = 0, Ce = null, Ye = null, ot = null, El = false, ua = false, ki = false, Sl = 0, sr = 0, sa = null, my = 0;
        function rt() {
          throw Error(i(321));
        }
        function Nu(e, t) {
          if (t === null) return false;
          for (var n = 0; n < t.length && n < e.length; n++) if (!Ut(e[n], t[n])) return false;
          return true;
        }
        function Ou(e, t, n, r, s, f) {
          return ei = f, Ce = t, t.memoizedState = null, t.updateQueue = null, t.lanes = 0, D.H = e === null || e.memoizedState === null ? Kd : Pd, ki = false, f = n(r, s), ki = false, ua && (f = gd(t, n, r, s)), hd(e), f;
        }
        function hd(e) {
          D.H = Dl;
          var t = Ye !== null && Ye.next !== null;
          if (ei = 0, ot = Ye = Ce = null, El = false, sr = 0, sa = null, t) throw Error(i(300));
          e === null || yt || (e = e.dependencies, e !== null && vl(e) && (yt = true));
        }
        function gd(e, t, n, r) {
          Ce = e;
          var s = 0;
          do {
            if (ua && (sa = null), sr = 0, ua = false, 25 <= s) throw Error(i(301));
            if (s += 1, ot = Ye = null, e.updateQueue != null) {
              var f = e.updateQueue;
              f.lastEffect = null, f.events = null, f.stores = null, f.memoCache != null && (f.memoCache.index = 0);
            }
            D.H = Ey, f = t(n, r);
          } while (ua);
          return f;
        }
        function vy() {
          var e = D.H, t = e.useState()[0];
          return t = typeof t.then == "function" ? cr(t) : t, e = e.useState()[0], (Ye !== null ? Ye.memoizedState : null) !== e && (Ce.flags |= 1024), t;
        }
        function zu() {
          var e = Sl !== 0;
          return Sl = 0, e;
        }
        function ku(e, t, n) {
          t.updateQueue = e.updateQueue, t.flags &= -2053, e.lanes &= ~n;
        }
        function Mu(e) {
          if (El) {
            for (e = e.memoizedState; e !== null; ) {
              var t = e.queue;
              t !== null && (t.pending = null), e = e.next;
            }
            El = false;
          }
          ei = 0, ot = Ye = Ce = null, ua = false, sr = Sl = 0, sa = null;
        }
        function Mt() {
          var e = {
            memoizedState: null,
            baseState: null,
            baseQueue: null,
            queue: null,
            next: null
          };
          return ot === null ? Ce.memoizedState = ot = e : ot = ot.next = e, ot;
        }
        function ut() {
          if (Ye === null) {
            var e = Ce.alternate;
            e = e !== null ? e.memoizedState : null;
          } else e = Ye.next;
          var t = ot === null ? Ce.memoizedState : ot.next;
          if (t !== null) ot = t, Ye = e;
          else {
            if (e === null) throw Ce.alternate === null ? Error(i(467)) : Error(i(310));
            Ye = e, e = {
              memoizedState: Ye.memoizedState,
              baseState: Ye.baseState,
              baseQueue: Ye.baseQueue,
              queue: Ye.queue,
              next: null
            }, ot === null ? Ce.memoizedState = ot = e : ot = ot.next = e;
          }
          return ot;
        }
        function Lu() {
          return {
            lastEffect: null,
            events: null,
            stores: null,
            memoCache: null
          };
        }
        function cr(e) {
          var t = sr;
          return sr += 1, sa === null && (sa = []), e = od(sa, e, t), t = Ce, (ot === null ? t.memoizedState : ot.next) === null && (t = t.alternate, D.H = t === null || t.memoizedState === null ? Kd : Pd), e;
        }
        function xl(e) {
          if (e !== null && typeof e == "object") {
            if (typeof e.then == "function") return cr(e);
            if (e.$$typeof === I) return Rt(e);
          }
          throw Error(i(438, String(e)));
        }
        function Gu(e) {
          var t = null, n = Ce.updateQueue;
          if (n !== null && (t = n.memoCache), t == null) {
            var r = Ce.alternate;
            r !== null && (r = r.updateQueue, r !== null && (r = r.memoCache, r != null && (t = {
              data: r.data.map(function(s) {
                return s.slice();
              }),
              index: 0
            })));
          }
          if (t == null && (t = {
            data: [],
            index: 0
          }), n === null && (n = Lu(), Ce.updateQueue = n), n.memoCache = t, n = t.data[t.index], n === void 0) for (n = t.data[t.index] = Array(e), r = 0; r < e; r++) n[r] = S;
          return t.index++, n;
        }
        function kn(e, t) {
          return typeof t == "function" ? t(e) : t;
        }
        function Tl(e) {
          var t = ut();
          return Uu(t, Ye, e);
        }
        function Uu(e, t, n) {
          var r = e.queue;
          if (r === null) throw Error(i(311));
          r.lastRenderedReducer = n;
          var s = e.baseQueue, f = r.pending;
          if (f !== null) {
            if (s !== null) {
              var v = s.next;
              s.next = f.next, f.next = v;
            }
            t.baseQueue = s = f, r.pending = null;
          }
          if (f = e.baseState, s === null) e.memoizedState = f;
          else {
            t = s.next;
            var b = v = null, A = null, j = t, Q = false;
            do {
              var W = j.lane & -536870913;
              if (W !== j.lane ? (Ge & W) === W : (ei & W) === W) {
                var F = j.revertLane;
                if (F === 0) A !== null && (A = A.next = {
                  lane: 0,
                  revertLane: 0,
                  action: j.action,
                  hasEagerState: j.hasEagerState,
                  eagerState: j.eagerState,
                  next: null
                }), W === ra && (Q = true);
                else if ((ei & F) === F) {
                  j = j.next, F === ra && (Q = true);
                  continue;
                } else W = {
                  lane: 0,
                  revertLane: j.revertLane,
                  action: j.action,
                  hasEagerState: j.hasEagerState,
                  eagerState: j.eagerState,
                  next: null
                }, A === null ? (b = A = W, v = f) : A = A.next = W, Ce.lanes |= F, oi |= F;
                W = j.action, ki && n(f, W), f = j.hasEagerState ? j.eagerState : n(f, W);
              } else F = {
                lane: W,
                revertLane: j.revertLane,
                action: j.action,
                hasEagerState: j.hasEagerState,
                eagerState: j.eagerState,
                next: null
              }, A === null ? (b = A = F, v = f) : A = A.next = F, Ce.lanes |= W, oi |= W;
              j = j.next;
            } while (j !== null && j !== t);
            if (A === null ? v = f : A.next = b, !Ut(f, e.memoizedState) && (yt = true, Q && (n = la, n !== null))) throw n;
            e.memoizedState = f, e.baseState = v, e.baseQueue = A, r.lastRenderedState = f;
          }
          return s === null && (r.lanes = 0), [
            e.memoizedState,
            r.dispatch
          ];
        }
        function ju(e) {
          var t = ut(), n = t.queue;
          if (n === null) throw Error(i(311));
          n.lastRenderedReducer = e;
          var r = n.dispatch, s = n.pending, f = t.memoizedState;
          if (s !== null) {
            n.pending = null;
            var v = s = s.next;
            do
              f = e(f, v.action), v = v.next;
            while (v !== s);
            Ut(f, t.memoizedState) || (yt = true), t.memoizedState = f, t.baseQueue === null && (t.baseState = f), n.lastRenderedState = f;
          }
          return [
            f,
            r
          ];
        }
        function md(e, t, n) {
          var r = Ce, s = ut(), f = Be;
          if (f) {
            if (n === void 0) throw Error(i(407));
            n = n();
          } else n = t();
          var v = !Ut((Ye || s).memoizedState, n);
          v && (s.memoizedState = n, yt = true), s = s.queue;
          var b = yd.bind(null, r, s, e);
          if (fr(2048, 8, b, [
            e
          ]), s.getSnapshot !== t || v || ot !== null && ot.memoizedState.tag & 1) {
            if (r.flags |= 2048, ca(9, Al(), pd.bind(null, r, s, n, t), null), Pe === null) throw Error(i(349));
            f || (ei & 124) !== 0 || vd(r, t, n);
          }
          return n;
        }
        function vd(e, t, n) {
          e.flags |= 16384, e = {
            getSnapshot: t,
            value: n
          }, t = Ce.updateQueue, t === null ? (t = Lu(), Ce.updateQueue = t, t.stores = [
            e
          ]) : (n = t.stores, n === null ? t.stores = [
            e
          ] : n.push(e));
        }
        function pd(e, t, n, r) {
          t.value = n, t.getSnapshot = r, bd(t) && _d(e);
        }
        function yd(e, t, n) {
          return n(function() {
            bd(t) && _d(e);
          });
        }
        function bd(e) {
          var t = e.getSnapshot;
          e = e.value;
          try {
            var n = t();
            return !Ut(e, n);
          } catch {
            return true;
          }
        }
        function _d(e) {
          var t = ta(e, 2);
          t !== null && Vt(t, e, 2);
        }
        function Bu(e) {
          var t = Mt();
          if (typeof e == "function") {
            var n = e;
            if (e = n(), ki) {
              vt(true);
              try {
                n();
              } finally {
                vt(false);
              }
            }
          }
          return t.memoizedState = t.baseState = e, t.queue = {
            pending: null,
            lanes: 0,
            dispatch: null,
            lastRenderedReducer: kn,
            lastRenderedState: e
          }, t;
        }
        function wd(e, t, n, r) {
          return e.baseState = n, Uu(e, Ye, typeof r == "function" ? r : kn);
        }
        function py(e, t, n, r, s) {
          if (Cl(e)) throw Error(i(485));
          if (e = t.action, e !== null) {
            var f = {
              payload: s,
              action: e,
              next: null,
              isTransition: true,
              status: "pending",
              value: null,
              reason: null,
              listeners: [],
              then: function(v) {
                f.listeners.push(v);
              }
            };
            D.T !== null ? n(true) : f.isTransition = false, r(f), n = t.pending, n === null ? (f.next = t.pending = f, Ed(t, f)) : (f.next = n.next, t.pending = n.next = f);
          }
        }
        function Ed(e, t) {
          var n = t.action, r = t.payload, s = e.state;
          if (t.isTransition) {
            var f = D.T, v = {};
            D.T = v;
            try {
              var b = n(s, r), A = D.S;
              A !== null && A(v, b), Sd(e, t, b);
            } catch (j) {
              Hu(e, t, j);
            } finally {
              D.T = f;
            }
          } else try {
            f = n(s, r), Sd(e, t, f);
          } catch (j) {
            Hu(e, t, j);
          }
        }
        function Sd(e, t, n) {
          n !== null && typeof n == "object" && typeof n.then == "function" ? n.then(function(r) {
            xd(e, t, r);
          }, function(r) {
            return Hu(e, t, r);
          }) : xd(e, t, n);
        }
        function xd(e, t, n) {
          t.status = "fulfilled", t.value = n, Td(t), e.state = n, t = e.pending, t !== null && (n = t.next, n === t ? e.pending = null : (n = n.next, t.next = n, Ed(e, n)));
        }
        function Hu(e, t, n) {
          var r = e.pending;
          if (e.pending = null, r !== null) {
            r = r.next;
            do
              t.status = "rejected", t.reason = n, Td(t), t = t.next;
            while (t !== r);
          }
          e.action = null;
        }
        function Td(e) {
          e = e.listeners;
          for (var t = 0; t < e.length; t++) (0, e[t])();
        }
        function Ad(e, t) {
          return t;
        }
        function Rd(e, t) {
          if (Be) {
            var n = Pe.formState;
            if (n !== null) {
              e: {
                var r = Ce;
                if (Be) {
                  if (it) {
                    t: {
                      for (var s = it, f = bn; s.nodeType !== 8; ) {
                        if (!f) {
                          s = null;
                          break t;
                        }
                        if (s = dn(s.nextSibling), s === null) {
                          s = null;
                          break t;
                        }
                      }
                      f = s.data, s = f === "F!" || f === "F" ? s : null;
                    }
                    if (s) {
                      it = dn(s.nextSibling), r = s.data === "F!";
                      break e;
                    }
                  }
                  Di(r);
                }
                r = false;
              }
              r && (t = n[0]);
            }
          }
          return n = Mt(), n.memoizedState = n.baseState = t, r = {
            pending: null,
            lanes: 0,
            dispatch: null,
            lastRenderedReducer: Ad,
            lastRenderedState: t
          }, n.queue = r, n = Xd.bind(null, Ce, r), r.dispatch = n, r = Bu(false), f = $u.bind(null, Ce, false, r.queue), r = Mt(), s = {
            state: t,
            dispatch: null,
            action: e,
            pending: null
          }, r.queue = s, n = py.bind(null, Ce, s, f, n), s.dispatch = n, r.memoizedState = e, [
            t,
            n,
            false
          ];
        }
        function Cd(e) {
          var t = ut();
          return Dd(t, Ye, e);
        }
        function Dd(e, t, n) {
          if (t = Uu(e, t, Ad)[0], e = Tl(kn)[0], typeof t == "object" && t !== null && typeof t.then == "function") try {
            var r = cr(t);
          } catch (v) {
            throw v === ar ? bl : v;
          }
          else r = t;
          t = ut();
          var s = t.queue, f = s.dispatch;
          return n !== t.memoizedState && (Ce.flags |= 2048, ca(9, Al(), yy.bind(null, s, n), null)), [
            r,
            f,
            e
          ];
        }
        function yy(e, t) {
          e.action = t;
        }
        function Nd(e) {
          var t = ut(), n = Ye;
          if (n !== null) return Dd(t, n, e);
          ut(), t = t.memoizedState, n = ut();
          var r = n.queue.dispatch;
          return n.memoizedState = e, [
            t,
            r,
            false
          ];
        }
        function ca(e, t, n, r) {
          return e = {
            tag: e,
            create: n,
            deps: r,
            inst: t,
            next: null
          }, t = Ce.updateQueue, t === null && (t = Lu(), Ce.updateQueue = t), n = t.lastEffect, n === null ? t.lastEffect = e.next = e : (r = n.next, n.next = e, e.next = r, t.lastEffect = e), e;
        }
        function Al() {
          return {
            destroy: void 0,
            resource: void 0
          };
        }
        function Od() {
          return ut().memoizedState;
        }
        function Rl(e, t, n, r) {
          var s = Mt();
          r = r === void 0 ? null : r, Ce.flags |= e, s.memoizedState = ca(1 | t, Al(), n, r);
        }
        function fr(e, t, n, r) {
          var s = ut();
          r = r === void 0 ? null : r;
          var f = s.memoizedState.inst;
          Ye !== null && r !== null && Nu(r, Ye.memoizedState.deps) ? s.memoizedState = ca(t, f, n, r) : (Ce.flags |= e, s.memoizedState = ca(1 | t, f, n, r));
        }
        function zd(e, t) {
          Rl(8390656, 8, e, t);
        }
        function kd(e, t) {
          fr(2048, 8, e, t);
        }
        function Md(e, t) {
          return fr(4, 2, e, t);
        }
        function Ld(e, t) {
          return fr(4, 4, e, t);
        }
        function Gd(e, t) {
          if (typeof t == "function") {
            e = e();
            var n = t(e);
            return function() {
              typeof n == "function" ? n() : t(null);
            };
          }
          if (t != null) return e = e(), t.current = e, function() {
            t.current = null;
          };
        }
        function Ud(e, t, n) {
          n = n != null ? n.concat([
            e
          ]) : null, fr(4, 4, Gd.bind(null, t, e), n);
        }
        function Fu() {
        }
        function jd(e, t) {
          var n = ut();
          t = t === void 0 ? null : t;
          var r = n.memoizedState;
          return t !== null && Nu(t, r[1]) ? r[0] : (n.memoizedState = [
            e,
            t
          ], e);
        }
        function Bd(e, t) {
          var n = ut();
          t = t === void 0 ? null : t;
          var r = n.memoizedState;
          if (t !== null && Nu(t, r[1])) return r[0];
          if (r = e(), ki) {
            vt(true);
            try {
              e();
            } finally {
              vt(false);
            }
          }
          return n.memoizedState = [
            r,
            t
          ], r;
        }
        function qu(e, t, n) {
          return n === void 0 || (ei & 1073741824) !== 0 ? e.memoizedState = t : (e.memoizedState = n, e = qh(), Ce.lanes |= e, oi |= e, n);
        }
        function Hd(e, t, n, r) {
          return Ut(n, t) ? n : oa.current !== null ? (e = qu(e, n, r), Ut(e, t) || (yt = true), e) : (ei & 42) === 0 ? (yt = true, e.memoizedState = n) : (e = qh(), Ce.lanes |= e, oi |= e, t);
        }
        function Fd(e, t, n, r, s) {
          var f = X.p;
          X.p = f !== 0 && 8 > f ? f : 8;
          var v = D.T, b = {};
          D.T = b, $u(e, false, t, n);
          try {
            var A = s(), j = D.S;
            if (j !== null && j(b, A), A !== null && typeof A == "object" && typeof A.then == "function") {
              var Q = gy(A, r);
              dr(e, t, Q, qt(e));
            } else dr(e, t, r, qt(e));
          } catch (W) {
            dr(e, t, {
              then: function() {
              },
              status: "rejected",
              reason: W
            }, qt());
          } finally {
            X.p = f, D.T = v;
          }
        }
        function by() {
        }
        function Vu(e, t, n, r) {
          if (e.tag !== 5) throw Error(i(476));
          var s = qd(e).queue;
          Fd(e, s, t, $, n === null ? by : function() {
            return Vd(e), n(r);
          });
        }
        function qd(e) {
          var t = e.memoizedState;
          if (t !== null) return t;
          t = {
            memoizedState: $,
            baseState: $,
            baseQueue: null,
            queue: {
              pending: null,
              lanes: 0,
              dispatch: null,
              lastRenderedReducer: kn,
              lastRenderedState: $
            },
            next: null
          };
          var n = {};
          return t.next = {
            memoizedState: n,
            baseState: n,
            baseQueue: null,
            queue: {
              pending: null,
              lanes: 0,
              dispatch: null,
              lastRenderedReducer: kn,
              lastRenderedState: n
            },
            next: null
          }, e.memoizedState = t, e = e.alternate, e !== null && (e.memoizedState = t), t;
        }
        function Vd(e) {
          var t = qd(e).next.queue;
          dr(e, t, {}, qt());
        }
        function Yu() {
          return Rt(Nr);
        }
        function Yd() {
          return ut().memoizedState;
        }
        function $d() {
          return ut().memoizedState;
        }
        function _y(e) {
          for (var t = e.return; t !== null; ) {
            switch (t.tag) {
              case 24:
              case 3:
                var n = qt();
                e = Wn(n);
                var r = Jn(t, e, n);
                r !== null && (Vt(r, t, n), lr(r, t, n)), t = {
                  cache: _u()
                }, e.payload = t;
                return;
            }
            t = t.return;
          }
        }
        function wy(e, t, n) {
          var r = qt();
          n = {
            lane: r,
            revertLane: 0,
            action: n,
            hasEagerState: false,
            eagerState: null,
            next: null
          }, Cl(e) ? Zd(t, n) : (n = cu(e, t, n, r), n !== null && (Vt(n, e, r), Qd(n, t, r)));
        }
        function Xd(e, t, n) {
          var r = qt();
          dr(e, t, n, r);
        }
        function dr(e, t, n, r) {
          var s = {
            lane: r,
            revertLane: 0,
            action: n,
            hasEagerState: false,
            eagerState: null,
            next: null
          };
          if (Cl(e)) Zd(t, s);
          else {
            var f = e.alternate;
            if (e.lanes === 0 && (f === null || f.lanes === 0) && (f = t.lastRenderedReducer, f !== null)) try {
              var v = t.lastRenderedState, b = f(v, n);
              if (s.hasEagerState = true, s.eagerState = b, Ut(b, v)) return fl(e, t, s, 0), Pe === null && cl(), false;
            } catch {
            } finally {
            }
            if (n = cu(e, t, s, r), n !== null) return Vt(n, e, r), Qd(n, t, r), true;
          }
          return false;
        }
        function $u(e, t, n, r) {
          if (r = {
            lane: 2,
            revertLane: Ss(),
            action: r,
            hasEagerState: false,
            eagerState: null,
            next: null
          }, Cl(e)) {
            if (t) throw Error(i(479));
          } else t = cu(e, n, r, 2), t !== null && Vt(t, e, 2);
        }
        function Cl(e) {
          var t = e.alternate;
          return e === Ce || t !== null && t === Ce;
        }
        function Zd(e, t) {
          ua = El = true;
          var n = e.pending;
          n === null ? t.next = t : (t.next = n.next, n.next = t), e.pending = t;
        }
        function Qd(e, t, n) {
          if ((n & 4194048) !== 0) {
            var r = t.lanes;
            r &= e.pendingLanes, n |= r, t.lanes = n, tf(e, n);
          }
        }
        var Dl = {
          readContext: Rt,
          use: xl,
          useCallback: rt,
          useContext: rt,
          useEffect: rt,
          useImperativeHandle: rt,
          useLayoutEffect: rt,
          useInsertionEffect: rt,
          useMemo: rt,
          useReducer: rt,
          useRef: rt,
          useState: rt,
          useDebugValue: rt,
          useDeferredValue: rt,
          useTransition: rt,
          useSyncExternalStore: rt,
          useId: rt,
          useHostTransitionStatus: rt,
          useFormState: rt,
          useActionState: rt,
          useOptimistic: rt,
          useMemoCache: rt,
          useCacheRefresh: rt
        }, Kd = {
          readContext: Rt,
          use: xl,
          useCallback: function(e, t) {
            return Mt().memoizedState = [
              e,
              t === void 0 ? null : t
            ], e;
          },
          useContext: Rt,
          useEffect: zd,
          useImperativeHandle: function(e, t, n) {
            n = n != null ? n.concat([
              e
            ]) : null, Rl(4194308, 4, Gd.bind(null, t, e), n);
          },
          useLayoutEffect: function(e, t) {
            return Rl(4194308, 4, e, t);
          },
          useInsertionEffect: function(e, t) {
            Rl(4, 2, e, t);
          },
          useMemo: function(e, t) {
            var n = Mt();
            t = t === void 0 ? null : t;
            var r = e();
            if (ki) {
              vt(true);
              try {
                e();
              } finally {
                vt(false);
              }
            }
            return n.memoizedState = [
              r,
              t
            ], r;
          },
          useReducer: function(e, t, n) {
            var r = Mt();
            if (n !== void 0) {
              var s = n(t);
              if (ki) {
                vt(true);
                try {
                  n(t);
                } finally {
                  vt(false);
                }
              }
            } else s = t;
            return r.memoizedState = r.baseState = s, e = {
              pending: null,
              lanes: 0,
              dispatch: null,
              lastRenderedReducer: e,
              lastRenderedState: s
            }, r.queue = e, e = e.dispatch = wy.bind(null, Ce, e), [
              r.memoizedState,
              e
            ];
          },
          useRef: function(e) {
            var t = Mt();
            return e = {
              current: e
            }, t.memoizedState = e;
          },
          useState: function(e) {
            e = Bu(e);
            var t = e.queue, n = Xd.bind(null, Ce, t);
            return t.dispatch = n, [
              e.memoizedState,
              n
            ];
          },
          useDebugValue: Fu,
          useDeferredValue: function(e, t) {
            var n = Mt();
            return qu(n, e, t);
          },
          useTransition: function() {
            var e = Bu(false);
            return e = Fd.bind(null, Ce, e.queue, true, false), Mt().memoizedState = e, [
              false,
              e
            ];
          },
          useSyncExternalStore: function(e, t, n) {
            var r = Ce, s = Mt();
            if (Be) {
              if (n === void 0) throw Error(i(407));
              n = n();
            } else {
              if (n = t(), Pe === null) throw Error(i(349));
              (Ge & 124) !== 0 || vd(r, t, n);
            }
            s.memoizedState = n;
            var f = {
              value: n,
              getSnapshot: t
            };
            return s.queue = f, zd(yd.bind(null, r, f, e), [
              e
            ]), r.flags |= 2048, ca(9, Al(), pd.bind(null, r, f, n, t), null), n;
          },
          useId: function() {
            var e = Mt(), t = Pe.identifierPrefix;
            if (Be) {
              var n = Nn, r = Dn;
              n = (r & ~(1 << 32 - wt(r) - 1)).toString(32) + n, t = "\xAB" + t + "R" + n, n = Sl++, 0 < n && (t += "H" + n.toString(32)), t += "\xBB";
            } else n = my++, t = "\xAB" + t + "r" + n.toString(32) + "\xBB";
            return e.memoizedState = t;
          },
          useHostTransitionStatus: Yu,
          useFormState: Rd,
          useActionState: Rd,
          useOptimistic: function(e) {
            var t = Mt();
            t.memoizedState = t.baseState = e;
            var n = {
              pending: null,
              lanes: 0,
              dispatch: null,
              lastRenderedReducer: null,
              lastRenderedState: null
            };
            return t.queue = n, t = $u.bind(null, Ce, true, n), n.dispatch = t, [
              e,
              t
            ];
          },
          useMemoCache: Gu,
          useCacheRefresh: function() {
            return Mt().memoizedState = _y.bind(null, Ce);
          }
        }, Pd = {
          readContext: Rt,
          use: xl,
          useCallback: jd,
          useContext: Rt,
          useEffect: kd,
          useImperativeHandle: Ud,
          useInsertionEffect: Md,
          useLayoutEffect: Ld,
          useMemo: Bd,
          useReducer: Tl,
          useRef: Od,
          useState: function() {
            return Tl(kn);
          },
          useDebugValue: Fu,
          useDeferredValue: function(e, t) {
            var n = ut();
            return Hd(n, Ye.memoizedState, e, t);
          },
          useTransition: function() {
            var e = Tl(kn)[0], t = ut().memoizedState;
            return [
              typeof e == "boolean" ? e : cr(e),
              t
            ];
          },
          useSyncExternalStore: md,
          useId: Yd,
          useHostTransitionStatus: Yu,
          useFormState: Cd,
          useActionState: Cd,
          useOptimistic: function(e, t) {
            var n = ut();
            return wd(n, Ye, e, t);
          },
          useMemoCache: Gu,
          useCacheRefresh: $d
        }, Ey = {
          readContext: Rt,
          use: xl,
          useCallback: jd,
          useContext: Rt,
          useEffect: kd,
          useImperativeHandle: Ud,
          useInsertionEffect: Md,
          useLayoutEffect: Ld,
          useMemo: Bd,
          useReducer: ju,
          useRef: Od,
          useState: function() {
            return ju(kn);
          },
          useDebugValue: Fu,
          useDeferredValue: function(e, t) {
            var n = ut();
            return Ye === null ? qu(n, e, t) : Hd(n, Ye.memoizedState, e, t);
          },
          useTransition: function() {
            var e = ju(kn)[0], t = ut().memoizedState;
            return [
              typeof e == "boolean" ? e : cr(e),
              t
            ];
          },
          useSyncExternalStore: md,
          useId: Yd,
          useHostTransitionStatus: Yu,
          useFormState: Nd,
          useActionState: Nd,
          useOptimistic: function(e, t) {
            var n = ut();
            return Ye !== null ? wd(n, Ye, e, t) : (n.baseState = e, [
              e,
              n.queue.dispatch
            ]);
          },
          useMemoCache: Gu,
          useCacheRefresh: $d
        }, fa = null, hr = 0;
        function Nl(e) {
          var t = hr;
          return hr += 1, fa === null && (fa = []), od(fa, e, t);
        }
        function gr(e, t) {
          t = t.props.ref, e.ref = t !== void 0 ? t : null;
        }
        function Ol(e, t) {
          throw t.$$typeof === _ ? Error(i(525)) : (e = Object.prototype.toString.call(t), Error(i(31, e === "[object Object]" ? "object with keys {" + Object.keys(t).join(", ") + "}" : e)));
        }
        function Id(e) {
          var t = e._init;
          return t(e._payload);
        }
        function Wd(e) {
          function t(O, C) {
            if (e) {
              var M = O.deletions;
              M === null ? (O.deletions = [
                C
              ], O.flags |= 16) : M.push(C);
            }
          }
          function n(O, C) {
            if (!e) return null;
            for (; C !== null; ) t(O, C), C = C.sibling;
            return null;
          }
          function r(O) {
            for (var C = /* @__PURE__ */ new Map(); O !== null; ) O.key !== null ? C.set(O.key, O) : C.set(O.index, O), O = O.sibling;
            return C;
          }
          function s(O, C) {
            return O = Cn(O, C), O.index = 0, O.sibling = null, O;
          }
          function f(O, C, M) {
            return O.index = M, e ? (M = O.alternate, M !== null ? (M = M.index, M < C ? (O.flags |= 67108866, C) : M) : (O.flags |= 67108866, C)) : (O.flags |= 1048576, C);
          }
          function v(O) {
            return e && O.alternate === null && (O.flags |= 67108866), O;
          }
          function b(O, C, M, P) {
            return C === null || C.tag !== 6 ? (C = du(M, O.mode, P), C.return = O, C) : (C = s(C, M), C.return = O, C);
          }
          function A(O, C, M, P) {
            var he = M.type;
            return he === z ? Q(O, C, M.props.children, P, M.key) : C !== null && (C.elementType === he || typeof he == "object" && he !== null && he.$$typeof === y && Id(he) === C.type) ? (C = s(C, M.props), gr(C, M), C.return = O, C) : (C = hl(M.type, M.key, M.props, null, O.mode, P), gr(C, M), C.return = O, C);
          }
          function j(O, C, M, P) {
            return C === null || C.tag !== 4 || C.stateNode.containerInfo !== M.containerInfo || C.stateNode.implementation !== M.implementation ? (C = hu(M, O.mode, P), C.return = O, C) : (C = s(C, M.children || []), C.return = O, C);
          }
          function Q(O, C, M, P, he) {
            return C === null || C.tag !== 7 ? (C = Ti(M, O.mode, P, he), C.return = O, C) : (C = s(C, M), C.return = O, C);
          }
          function W(O, C, M) {
            if (typeof C == "string" && C !== "" || typeof C == "number" || typeof C == "bigint") return C = du("" + C, O.mode, M), C.return = O, C;
            if (typeof C == "object" && C !== null) {
              switch (C.$$typeof) {
                case E:
                  return M = hl(C.type, C.key, C.props, null, O.mode, M), gr(M, C), M.return = O, M;
                case R:
                  return C = hu(C, O.mode, M), C.return = O, C;
                case y:
                  var P = C._init;
                  return C = P(C._payload), W(O, C, M);
              }
              if (oe(C) || L(C)) return C = Ti(C, O.mode, M, null), C.return = O, C;
              if (typeof C.then == "function") return W(O, Nl(C), M);
              if (C.$$typeof === I) return W(O, pl(O, C), M);
              Ol(O, C);
            }
            return null;
          }
          function F(O, C, M, P) {
            var he = C !== null ? C.key : null;
            if (typeof M == "string" && M !== "" || typeof M == "number" || typeof M == "bigint") return he !== null ? null : b(O, C, "" + M, P);
            if (typeof M == "object" && M !== null) {
              switch (M.$$typeof) {
                case E:
                  return M.key === he ? A(O, C, M, P) : null;
                case R:
                  return M.key === he ? j(O, C, M, P) : null;
                case y:
                  return he = M._init, M = he(M._payload), F(O, C, M, P);
              }
              if (oe(M) || L(M)) return he !== null ? null : Q(O, C, M, P, null);
              if (typeof M.then == "function") return F(O, C, Nl(M), P);
              if (M.$$typeof === I) return F(O, C, pl(O, M), P);
              Ol(O, M);
            }
            return null;
          }
          function q(O, C, M, P, he) {
            if (typeof P == "string" && P !== "" || typeof P == "number" || typeof P == "bigint") return O = O.get(M) || null, b(C, O, "" + P, he);
            if (typeof P == "object" && P !== null) {
              switch (P.$$typeof) {
                case E:
                  return O = O.get(P.key === null ? M : P.key) || null, A(C, O, P, he);
                case R:
                  return O = O.get(P.key === null ? M : P.key) || null, j(C, O, P, he);
                case y:
                  var Ne = P._init;
                  return P = Ne(P._payload), q(O, C, M, P, he);
              }
              if (oe(P) || L(P)) return O = O.get(M) || null, Q(C, O, P, he, null);
              if (typeof P.then == "function") return q(O, C, M, Nl(P), he);
              if (P.$$typeof === I) return q(O, C, M, pl(C, P), he);
              Ol(C, P);
            }
            return null;
          }
          function xe(O, C, M, P) {
            for (var he = null, Ne = null, ye = C, Se = C = 0, _t = null; ye !== null && Se < M.length; Se++) {
              ye.index > Se ? (_t = ye, ye = null) : _t = ye.sibling;
              var Ue = F(O, ye, M[Se], P);
              if (Ue === null) {
                ye === null && (ye = _t);
                break;
              }
              e && ye && Ue.alternate === null && t(O, ye), C = f(Ue, C, Se), Ne === null ? he = Ue : Ne.sibling = Ue, Ne = Ue, ye = _t;
            }
            if (Se === M.length) return n(O, ye), Be && Ri(O, Se), he;
            if (ye === null) {
              for (; Se < M.length; Se++) ye = W(O, M[Se], P), ye !== null && (C = f(ye, C, Se), Ne === null ? he = ye : Ne.sibling = ye, Ne = ye);
              return Be && Ri(O, Se), he;
            }
            for (ye = r(ye); Se < M.length; Se++) _t = q(ye, O, Se, M[Se], P), _t !== null && (e && _t.alternate !== null && ye.delete(_t.key === null ? Se : _t.key), C = f(_t, C, Se), Ne === null ? he = _t : Ne.sibling = _t, Ne = _t);
            return e && ye.forEach(function(vi) {
              return t(O, vi);
            }), Be && Ri(O, Se), he;
          }
          function we(O, C, M, P) {
            if (M == null) throw Error(i(151));
            for (var he = null, Ne = null, ye = C, Se = C = 0, _t = null, Ue = M.next(); ye !== null && !Ue.done; Se++, Ue = M.next()) {
              ye.index > Se ? (_t = ye, ye = null) : _t = ye.sibling;
              var vi = F(O, ye, Ue.value, P);
              if (vi === null) {
                ye === null && (ye = _t);
                break;
              }
              e && ye && vi.alternate === null && t(O, ye), C = f(vi, C, Se), Ne === null ? he = vi : Ne.sibling = vi, Ne = vi, ye = _t;
            }
            if (Ue.done) return n(O, ye), Be && Ri(O, Se), he;
            if (ye === null) {
              for (; !Ue.done; Se++, Ue = M.next()) Ue = W(O, Ue.value, P), Ue !== null && (C = f(Ue, C, Se), Ne === null ? he = Ue : Ne.sibling = Ue, Ne = Ue);
              return Be && Ri(O, Se), he;
            }
            for (ye = r(ye); !Ue.done; Se++, Ue = M.next()) Ue = q(ye, O, Se, Ue.value, P), Ue !== null && (e && Ue.alternate !== null && ye.delete(Ue.key === null ? Se : Ue.key), C = f(Ue, C, Se), Ne === null ? he = Ue : Ne.sibling = Ue, Ne = Ue);
            return e && ye.forEach(function(Sb) {
              return t(O, Sb);
            }), Be && Ri(O, Se), he;
          }
          function Xe(O, C, M, P) {
            if (typeof M == "object" && M !== null && M.type === z && M.key === null && (M = M.props.children), typeof M == "object" && M !== null) {
              switch (M.$$typeof) {
                case E:
                  e: {
                    for (var he = M.key; C !== null; ) {
                      if (C.key === he) {
                        if (he = M.type, he === z) {
                          if (C.tag === 7) {
                            n(O, C.sibling), P = s(C, M.props.children), P.return = O, O = P;
                            break e;
                          }
                        } else if (C.elementType === he || typeof he == "object" && he !== null && he.$$typeof === y && Id(he) === C.type) {
                          n(O, C.sibling), P = s(C, M.props), gr(P, M), P.return = O, O = P;
                          break e;
                        }
                        n(O, C);
                        break;
                      } else t(O, C);
                      C = C.sibling;
                    }
                    M.type === z ? (P = Ti(M.props.children, O.mode, P, M.key), P.return = O, O = P) : (P = hl(M.type, M.key, M.props, null, O.mode, P), gr(P, M), P.return = O, O = P);
                  }
                  return v(O);
                case R:
                  e: {
                    for (he = M.key; C !== null; ) {
                      if (C.key === he) if (C.tag === 4 && C.stateNode.containerInfo === M.containerInfo && C.stateNode.implementation === M.implementation) {
                        n(O, C.sibling), P = s(C, M.children || []), P.return = O, O = P;
                        break e;
                      } else {
                        n(O, C);
                        break;
                      }
                      else t(O, C);
                      C = C.sibling;
                    }
                    P = hu(M, O.mode, P), P.return = O, O = P;
                  }
                  return v(O);
                case y:
                  return he = M._init, M = he(M._payload), Xe(O, C, M, P);
              }
              if (oe(M)) return xe(O, C, M, P);
              if (L(M)) {
                if (he = L(M), typeof he != "function") throw Error(i(150));
                return M = he.call(M), we(O, C, M, P);
              }
              if (typeof M.then == "function") return Xe(O, C, Nl(M), P);
              if (M.$$typeof === I) return Xe(O, C, pl(O, M), P);
              Ol(O, M);
            }
            return typeof M == "string" && M !== "" || typeof M == "number" || typeof M == "bigint" ? (M = "" + M, C !== null && C.tag === 6 ? (n(O, C.sibling), P = s(C, M), P.return = O, O = P) : (n(O, C), P = du(M, O.mode, P), P.return = O, O = P), v(O)) : n(O, C);
          }
          return function(O, C, M, P) {
            try {
              hr = 0;
              var he = Xe(O, C, M, P);
              return fa = null, he;
            } catch (ye) {
              if (ye === ar || ye === bl) throw ye;
              var Ne = jt(29, ye, null, O.mode);
              return Ne.lanes = P, Ne.return = O, Ne;
            } finally {
            }
          };
        }
        var da = Wd(true), Jd = Wd(false), en = k(null), _n = null;
        function ti(e) {
          var t = e.alternate;
          U(ft, ft.current & 1), U(en, e), _n === null && (t === null || oa.current !== null || t.memoizedState !== null) && (_n = e);
        }
        function eh(e) {
          if (e.tag === 22) {
            if (U(ft, ft.current), U(en, e), _n === null) {
              var t = e.alternate;
              t !== null && t.memoizedState !== null && (_n = e);
            }
          } else ni();
        }
        function ni() {
          U(ft, ft.current), U(en, en.current);
        }
        function Mn(e) {
          K(en), _n === e && (_n = null), K(ft);
        }
        var ft = k(0);
        function zl(e) {
          for (var t = e; t !== null; ) {
            if (t.tag === 13) {
              var n = t.memoizedState;
              if (n !== null && (n = n.dehydrated, n === null || n.data === "$?" || Ls(n))) return t;
            } else if (t.tag === 19 && t.memoizedProps.revealOrder !== void 0) {
              if ((t.flags & 128) !== 0) return t;
            } else if (t.child !== null) {
              t.child.return = t, t = t.child;
              continue;
            }
            if (t === e) break;
            for (; t.sibling === null; ) {
              if (t.return === null || t.return === e) return null;
              t = t.return;
            }
            t.sibling.return = t.return, t = t.sibling;
          }
          return null;
        }
        function Xu(e, t, n, r) {
          t = e.memoizedState, n = n(r, t), n = n == null ? t : p({}, t, n), e.memoizedState = n, e.lanes === 0 && (e.updateQueue.baseState = n);
        }
        var Zu = {
          enqueueSetState: function(e, t, n) {
            e = e._reactInternals;
            var r = qt(), s = Wn(r);
            s.payload = t, n != null && (s.callback = n), t = Jn(e, s, r), t !== null && (Vt(t, e, r), lr(t, e, r));
          },
          enqueueReplaceState: function(e, t, n) {
            e = e._reactInternals;
            var r = qt(), s = Wn(r);
            s.tag = 1, s.payload = t, n != null && (s.callback = n), t = Jn(e, s, r), t !== null && (Vt(t, e, r), lr(t, e, r));
          },
          enqueueForceUpdate: function(e, t) {
            e = e._reactInternals;
            var n = qt(), r = Wn(n);
            r.tag = 2, t != null && (r.callback = t), t = Jn(e, r, n), t !== null && (Vt(t, e, n), lr(t, e, n));
          }
        };
        function th(e, t, n, r, s, f, v) {
          return e = e.stateNode, typeof e.shouldComponentUpdate == "function" ? e.shouldComponentUpdate(r, f, v) : t.prototype && t.prototype.isPureReactComponent ? !Pa(n, r) || !Pa(s, f) : true;
        }
        function nh(e, t, n, r) {
          e = t.state, typeof t.componentWillReceiveProps == "function" && t.componentWillReceiveProps(n, r), typeof t.UNSAFE_componentWillReceiveProps == "function" && t.UNSAFE_componentWillReceiveProps(n, r), t.state !== e && Zu.enqueueReplaceState(t, t.state, null);
        }
        function Mi(e, t) {
          var n = t;
          if ("ref" in t) {
            n = {};
            for (var r in t) r !== "ref" && (n[r] = t[r]);
          }
          if (e = e.defaultProps) {
            n === t && (n = p({}, n));
            for (var s in e) n[s] === void 0 && (n[s] = e[s]);
          }
          return n;
        }
        var kl = typeof reportError == "function" ? reportError : function(e) {
          if (typeof window == "object" && typeof window.ErrorEvent == "function") {
            var t = new window.ErrorEvent("error", {
              bubbles: true,
              cancelable: true,
              message: typeof e == "object" && e !== null && typeof e.message == "string" ? String(e.message) : String(e),
              error: e
            });
            if (!window.dispatchEvent(t)) return;
          } else if (typeof process == "object" && typeof process.emit == "function") {
            process.emit("uncaughtException", e);
            return;
          }
          console.error(e);
        };
        function ih(e) {
          kl(e);
        }
        function ah(e) {
          console.error(e);
        }
        function rh(e) {
          kl(e);
        }
        function Ml(e, t) {
          try {
            var n = e.onUncaughtError;
            n(t.value, {
              componentStack: t.stack
            });
          } catch (r) {
            setTimeout(function() {
              throw r;
            });
          }
        }
        function lh(e, t, n) {
          try {
            var r = e.onCaughtError;
            r(n.value, {
              componentStack: n.stack,
              errorBoundary: t.tag === 1 ? t.stateNode : null
            });
          } catch (s) {
            setTimeout(function() {
              throw s;
            });
          }
        }
        function Qu(e, t, n) {
          return n = Wn(n), n.tag = 3, n.payload = {
            element: null
          }, n.callback = function() {
            Ml(e, t);
          }, n;
        }
        function oh(e) {
          return e = Wn(e), e.tag = 3, e;
        }
        function uh(e, t, n, r) {
          var s = n.type.getDerivedStateFromError;
          if (typeof s == "function") {
            var f = r.value;
            e.payload = function() {
              return s(f);
            }, e.callback = function() {
              lh(t, n, r);
            };
          }
          var v = n.stateNode;
          v !== null && typeof v.componentDidCatch == "function" && (e.callback = function() {
            lh(t, n, r), typeof s != "function" && (ui === null ? ui = /* @__PURE__ */ new Set([
              this
            ]) : ui.add(this));
            var b = r.stack;
            this.componentDidCatch(r.value, {
              componentStack: b !== null ? b : ""
            });
          });
        }
        function Sy(e, t, n, r, s) {
          if (n.flags |= 32768, r !== null && typeof r == "object" && typeof r.then == "function") {
            if (t = n.alternate, t !== null && tr(t, n, s, true), n = en.current, n !== null) {
              switch (n.tag) {
                case 13:
                  return _n === null ? ys() : n.alternate === null && at === 0 && (at = 3), n.flags &= -257, n.flags |= 65536, n.lanes = s, r === Su ? n.flags |= 16384 : (t = n.updateQueue, t === null ? n.updateQueue = /* @__PURE__ */ new Set([
                    r
                  ]) : t.add(r), _s(e, r, s)), false;
                case 22:
                  return n.flags |= 65536, r === Su ? n.flags |= 16384 : (t = n.updateQueue, t === null ? (t = {
                    transitions: null,
                    markerInstances: null,
                    retryQueue: /* @__PURE__ */ new Set([
                      r
                    ])
                  }, n.updateQueue = t) : (n = t.retryQueue, n === null ? t.retryQueue = /* @__PURE__ */ new Set([
                    r
                  ]) : n.add(r)), _s(e, r, s)), false;
              }
              throw Error(i(435, n.tag));
            }
            return _s(e, r, s), ys(), false;
          }
          if (Be) return t = en.current, t !== null ? ((t.flags & 65536) === 0 && (t.flags |= 256), t.flags |= 65536, t.lanes = s, r !== vu && (e = Error(i(422), {
            cause: r
          }), er(Pt(e, n)))) : (r !== vu && (t = Error(i(423), {
            cause: r
          }), er(Pt(t, n))), e = e.current.alternate, e.flags |= 65536, s &= -s, e.lanes |= s, r = Pt(r, n), s = Qu(e.stateNode, r, s), Au(e, s), at !== 4 && (at = 2)), false;
          var f = Error(i(520), {
            cause: r
          });
          if (f = Pt(f, n), wr === null ? wr = [
            f
          ] : wr.push(f), at !== 4 && (at = 2), t === null) return true;
          r = Pt(r, n), n = t;
          do {
            switch (n.tag) {
              case 3:
                return n.flags |= 65536, e = s & -s, n.lanes |= e, e = Qu(n.stateNode, r, e), Au(n, e), false;
              case 1:
                if (t = n.type, f = n.stateNode, (n.flags & 128) === 0 && (typeof t.getDerivedStateFromError == "function" || f !== null && typeof f.componentDidCatch == "function" && (ui === null || !ui.has(f)))) return n.flags |= 65536, s &= -s, n.lanes |= s, s = oh(s), uh(s, e, n, r), Au(n, s), false;
            }
            n = n.return;
          } while (n !== null);
          return false;
        }
        var sh = Error(i(461)), yt = false;
        function Et(e, t, n, r) {
          t.child = e === null ? Jd(t, null, n, r) : da(t, e.child, n, r);
        }
        function ch(e, t, n, r, s) {
          n = n.render;
          var f = t.ref;
          if ("ref" in r) {
            var v = {};
            for (var b in r) b !== "ref" && (v[b] = r[b]);
          } else v = r;
          return Oi(t), r = Ou(e, t, n, v, f, s), b = zu(), e !== null && !yt ? (ku(e, t, s), Ln(e, t, s)) : (Be && b && gu(t), t.flags |= 1, Et(e, t, r, s), t.child);
        }
        function fh(e, t, n, r, s) {
          if (e === null) {
            var f = n.type;
            return typeof f == "function" && !fu(f) && f.defaultProps === void 0 && n.compare === null ? (t.tag = 15, t.type = f, dh(e, t, f, r, s)) : (e = hl(n.type, null, r, t, t.mode, s), e.ref = t.ref, e.return = t, t.child = e);
          }
          if (f = e.child, !ns(e, s)) {
            var v = f.memoizedProps;
            if (n = n.compare, n = n !== null ? n : Pa, n(v, r) && e.ref === t.ref) return Ln(e, t, s);
          }
          return t.flags |= 1, e = Cn(f, r), e.ref = t.ref, e.return = t, t.child = e;
        }
        function dh(e, t, n, r, s) {
          if (e !== null) {
            var f = e.memoizedProps;
            if (Pa(f, r) && e.ref === t.ref) if (yt = false, t.pendingProps = r = f, ns(e, s)) (e.flags & 131072) !== 0 && (yt = true);
            else return t.lanes = e.lanes, Ln(e, t, s);
          }
          return Ku(e, t, n, r, s);
        }
        function hh(e, t, n) {
          var r = t.pendingProps, s = r.children, f = e !== null ? e.memoizedState : null;
          if (r.mode === "hidden") {
            if ((t.flags & 128) !== 0) {
              if (r = f !== null ? f.baseLanes | n : n, e !== null) {
                for (s = t.child = e.child, f = 0; s !== null; ) f = f | s.lanes | s.childLanes, s = s.sibling;
                t.childLanes = f & ~r;
              } else t.childLanes = 0, t.child = null;
              return gh(e, t, r, n);
            }
            if ((n & 536870912) !== 0) t.memoizedState = {
              baseLanes: 0,
              cachePool: null
            }, e !== null && yl(t, f !== null ? f.cachePool : null), f !== null ? dd(t, f) : Cu(), eh(t);
            else return t.lanes = t.childLanes = 536870912, gh(e, t, f !== null ? f.baseLanes | n : n, n);
          } else f !== null ? (yl(t, f.cachePool), dd(t, f), ni(), t.memoizedState = null) : (e !== null && yl(t, null), Cu(), ni());
          return Et(e, t, s, n), t.child;
        }
        function gh(e, t, n, r) {
          var s = Eu();
          return s = s === null ? null : {
            parent: ct._currentValue,
            pool: s
          }, t.memoizedState = {
            baseLanes: n,
            cachePool: s
          }, e !== null && yl(t, null), Cu(), eh(t), e !== null && tr(e, t, r, true), null;
        }
        function Ll(e, t) {
          var n = t.ref;
          if (n === null) e !== null && e.ref !== null && (t.flags |= 4194816);
          else {
            if (typeof n != "function" && typeof n != "object") throw Error(i(284));
            (e === null || e.ref !== n) && (t.flags |= 4194816);
          }
        }
        function Ku(e, t, n, r, s) {
          return Oi(t), n = Ou(e, t, n, r, void 0, s), r = zu(), e !== null && !yt ? (ku(e, t, s), Ln(e, t, s)) : (Be && r && gu(t), t.flags |= 1, Et(e, t, n, s), t.child);
        }
        function mh(e, t, n, r, s, f) {
          return Oi(t), t.updateQueue = null, n = gd(t, r, n, s), hd(e), r = zu(), e !== null && !yt ? (ku(e, t, f), Ln(e, t, f)) : (Be && r && gu(t), t.flags |= 1, Et(e, t, n, f), t.child);
        }
        function vh(e, t, n, r, s) {
          if (Oi(t), t.stateNode === null) {
            var f = na, v = n.contextType;
            typeof v == "object" && v !== null && (f = Rt(v)), f = new n(r, f), t.memoizedState = f.state !== null && f.state !== void 0 ? f.state : null, f.updater = Zu, t.stateNode = f, f._reactInternals = t, f = t.stateNode, f.props = r, f.state = t.memoizedState, f.refs = {}, xu(t), v = n.contextType, f.context = typeof v == "object" && v !== null ? Rt(v) : na, f.state = t.memoizedState, v = n.getDerivedStateFromProps, typeof v == "function" && (Xu(t, n, v, r), f.state = t.memoizedState), typeof n.getDerivedStateFromProps == "function" || typeof f.getSnapshotBeforeUpdate == "function" || typeof f.UNSAFE_componentWillMount != "function" && typeof f.componentWillMount != "function" || (v = f.state, typeof f.componentWillMount == "function" && f.componentWillMount(), typeof f.UNSAFE_componentWillMount == "function" && f.UNSAFE_componentWillMount(), v !== f.state && Zu.enqueueReplaceState(f, f.state, null), ur(t, r, f, s), or(), f.state = t.memoizedState), typeof f.componentDidMount == "function" && (t.flags |= 4194308), r = true;
          } else if (e === null) {
            f = t.stateNode;
            var b = t.memoizedProps, A = Mi(n, b);
            f.props = A;
            var j = f.context, Q = n.contextType;
            v = na, typeof Q == "object" && Q !== null && (v = Rt(Q));
            var W = n.getDerivedStateFromProps;
            Q = typeof W == "function" || typeof f.getSnapshotBeforeUpdate == "function", b = t.pendingProps !== b, Q || typeof f.UNSAFE_componentWillReceiveProps != "function" && typeof f.componentWillReceiveProps != "function" || (b || j !== v) && nh(t, f, r, v), In = false;
            var F = t.memoizedState;
            f.state = F, ur(t, r, f, s), or(), j = t.memoizedState, b || F !== j || In ? (typeof W == "function" && (Xu(t, n, W, r), j = t.memoizedState), (A = In || th(t, n, A, r, F, j, v)) ? (Q || typeof f.UNSAFE_componentWillMount != "function" && typeof f.componentWillMount != "function" || (typeof f.componentWillMount == "function" && f.componentWillMount(), typeof f.UNSAFE_componentWillMount == "function" && f.UNSAFE_componentWillMount()), typeof f.componentDidMount == "function" && (t.flags |= 4194308)) : (typeof f.componentDidMount == "function" && (t.flags |= 4194308), t.memoizedProps = r, t.memoizedState = j), f.props = r, f.state = j, f.context = v, r = A) : (typeof f.componentDidMount == "function" && (t.flags |= 4194308), r = false);
          } else {
            f = t.stateNode, Tu(e, t), v = t.memoizedProps, Q = Mi(n, v), f.props = Q, W = t.pendingProps, F = f.context, j = n.contextType, A = na, typeof j == "object" && j !== null && (A = Rt(j)), b = n.getDerivedStateFromProps, (j = typeof b == "function" || typeof f.getSnapshotBeforeUpdate == "function") || typeof f.UNSAFE_componentWillReceiveProps != "function" && typeof f.componentWillReceiveProps != "function" || (v !== W || F !== A) && nh(t, f, r, A), In = false, F = t.memoizedState, f.state = F, ur(t, r, f, s), or();
            var q = t.memoizedState;
            v !== W || F !== q || In || e !== null && e.dependencies !== null && vl(e.dependencies) ? (typeof b == "function" && (Xu(t, n, b, r), q = t.memoizedState), (Q = In || th(t, n, Q, r, F, q, A) || e !== null && e.dependencies !== null && vl(e.dependencies)) ? (j || typeof f.UNSAFE_componentWillUpdate != "function" && typeof f.componentWillUpdate != "function" || (typeof f.componentWillUpdate == "function" && f.componentWillUpdate(r, q, A), typeof f.UNSAFE_componentWillUpdate == "function" && f.UNSAFE_componentWillUpdate(r, q, A)), typeof f.componentDidUpdate == "function" && (t.flags |= 4), typeof f.getSnapshotBeforeUpdate == "function" && (t.flags |= 1024)) : (typeof f.componentDidUpdate != "function" || v === e.memoizedProps && F === e.memoizedState || (t.flags |= 4), typeof f.getSnapshotBeforeUpdate != "function" || v === e.memoizedProps && F === e.memoizedState || (t.flags |= 1024), t.memoizedProps = r, t.memoizedState = q), f.props = r, f.state = q, f.context = A, r = Q) : (typeof f.componentDidUpdate != "function" || v === e.memoizedProps && F === e.memoizedState || (t.flags |= 4), typeof f.getSnapshotBeforeUpdate != "function" || v === e.memoizedProps && F === e.memoizedState || (t.flags |= 1024), r = false);
          }
          return f = r, Ll(e, t), r = (t.flags & 128) !== 0, f || r ? (f = t.stateNode, n = r && typeof n.getDerivedStateFromError != "function" ? null : f.render(), t.flags |= 1, e !== null && r ? (t.child = da(t, e.child, null, s), t.child = da(t, null, n, s)) : Et(e, t, n, s), t.memoizedState = f.state, e = t.child) : e = Ln(e, t, s), e;
        }
        function ph(e, t, n, r) {
          return Ja(), t.flags |= 256, Et(e, t, n, r), t.child;
        }
        var Pu = {
          dehydrated: null,
          treeContext: null,
          retryLane: 0,
          hydrationErrors: null
        };
        function Iu(e) {
          return {
            baseLanes: e,
            cachePool: ad()
          };
        }
        function Wu(e, t, n) {
          return e = e !== null ? e.childLanes & ~n : 0, t && (e |= tn), e;
        }
        function yh(e, t, n) {
          var r = t.pendingProps, s = false, f = (t.flags & 128) !== 0, v;
          if ((v = f) || (v = e !== null && e.memoizedState === null ? false : (ft.current & 2) !== 0), v && (s = true, t.flags &= -129), v = (t.flags & 32) !== 0, t.flags &= -33, e === null) {
            if (Be) {
              if (s ? ti(t) : ni(), Be) {
                var b = it, A;
                if (A = b) {
                  e: {
                    for (A = b, b = bn; A.nodeType !== 8; ) {
                      if (!b) {
                        b = null;
                        break e;
                      }
                      if (A = dn(A.nextSibling), A === null) {
                        b = null;
                        break e;
                      }
                    }
                    b = A;
                  }
                  b !== null ? (t.memoizedState = {
                    dehydrated: b,
                    treeContext: Ai !== null ? {
                      id: Dn,
                      overflow: Nn
                    } : null,
                    retryLane: 536870912,
                    hydrationErrors: null
                  }, A = jt(18, null, null, 0), A.stateNode = b, A.return = t, t.child = A, Ot = t, it = null, A = true) : A = false;
                }
                A || Di(t);
              }
              if (b = t.memoizedState, b !== null && (b = b.dehydrated, b !== null)) return Ls(b) ? t.lanes = 32 : t.lanes = 536870912, null;
              Mn(t);
            }
            return b = r.children, r = r.fallback, s ? (ni(), s = t.mode, b = Gl({
              mode: "hidden",
              children: b
            }, s), r = Ti(r, s, n, null), b.return = t, r.return = t, b.sibling = r, t.child = b, s = t.child, s.memoizedState = Iu(n), s.childLanes = Wu(e, v, n), t.memoizedState = Pu, r) : (ti(t), Ju(t, b));
          }
          if (A = e.memoizedState, A !== null && (b = A.dehydrated, b !== null)) {
            if (f) t.flags & 256 ? (ti(t), t.flags &= -257, t = es(e, t, n)) : t.memoizedState !== null ? (ni(), t.child = e.child, t.flags |= 128, t = null) : (ni(), s = r.fallback, b = t.mode, r = Gl({
              mode: "visible",
              children: r.children
            }, b), s = Ti(s, b, n, null), s.flags |= 2, r.return = t, s.return = t, r.sibling = s, t.child = r, da(t, e.child, null, n), r = t.child, r.memoizedState = Iu(n), r.childLanes = Wu(e, v, n), t.memoizedState = Pu, t = s);
            else if (ti(t), Ls(b)) {
              if (v = b.nextSibling && b.nextSibling.dataset, v) var j = v.dgst;
              v = j, r = Error(i(419)), r.stack = "", r.digest = v, er({
                value: r,
                source: null,
                stack: null
              }), t = es(e, t, n);
            } else if (yt || tr(e, t, n, false), v = (n & e.childLanes) !== 0, yt || v) {
              if (v = Pe, v !== null && (r = n & -n, r = (r & 42) !== 0 ? 1 : Go(r), r = (r & (v.suspendedLanes | n)) !== 0 ? 0 : r, r !== 0 && r !== A.retryLane)) throw A.retryLane = r, ta(e, r), Vt(v, e, r), sh;
              b.data === "$?" || ys(), t = es(e, t, n);
            } else b.data === "$?" ? (t.flags |= 192, t.child = e.child, t = null) : (e = A.treeContext, it = dn(b.nextSibling), Ot = t, Be = true, Ci = null, bn = false, e !== null && (Wt[Jt++] = Dn, Wt[Jt++] = Nn, Wt[Jt++] = Ai, Dn = e.id, Nn = e.overflow, Ai = t), t = Ju(t, r.children), t.flags |= 4096);
            return t;
          }
          return s ? (ni(), s = r.fallback, b = t.mode, A = e.child, j = A.sibling, r = Cn(A, {
            mode: "hidden",
            children: r.children
          }), r.subtreeFlags = A.subtreeFlags & 65011712, j !== null ? s = Cn(j, s) : (s = Ti(s, b, n, null), s.flags |= 2), s.return = t, r.return = t, r.sibling = s, t.child = r, r = s, s = t.child, b = e.child.memoizedState, b === null ? b = Iu(n) : (A = b.cachePool, A !== null ? (j = ct._currentValue, A = A.parent !== j ? {
            parent: j,
            pool: j
          } : A) : A = ad(), b = {
            baseLanes: b.baseLanes | n,
            cachePool: A
          }), s.memoizedState = b, s.childLanes = Wu(e, v, n), t.memoizedState = Pu, r) : (ti(t), n = e.child, e = n.sibling, n = Cn(n, {
            mode: "visible",
            children: r.children
          }), n.return = t, n.sibling = null, e !== null && (v = t.deletions, v === null ? (t.deletions = [
            e
          ], t.flags |= 16) : v.push(e)), t.child = n, t.memoizedState = null, n);
        }
        function Ju(e, t) {
          return t = Gl({
            mode: "visible",
            children: t
          }, e.mode), t.return = e, e.child = t;
        }
        function Gl(e, t) {
          return e = jt(22, e, null, t), e.lanes = 0, e.stateNode = {
            _visibility: 1,
            _pendingMarkers: null,
            _retryCache: null,
            _transitions: null
          }, e;
        }
        function es(e, t, n) {
          return da(t, e.child, null, n), e = Ju(t, t.pendingProps.children), e.flags |= 2, t.memoizedState = null, e;
        }
        function bh(e, t, n) {
          e.lanes |= t;
          var r = e.alternate;
          r !== null && (r.lanes |= t), yu(e.return, t, n);
        }
        function ts(e, t, n, r, s) {
          var f = e.memoizedState;
          f === null ? e.memoizedState = {
            isBackwards: t,
            rendering: null,
            renderingStartTime: 0,
            last: r,
            tail: n,
            tailMode: s
          } : (f.isBackwards = t, f.rendering = null, f.renderingStartTime = 0, f.last = r, f.tail = n, f.tailMode = s);
        }
        function _h(e, t, n) {
          var r = t.pendingProps, s = r.revealOrder, f = r.tail;
          if (Et(e, t, r.children, n), r = ft.current, (r & 2) !== 0) r = r & 1 | 2, t.flags |= 128;
          else {
            if (e !== null && (e.flags & 128) !== 0) e: for (e = t.child; e !== null; ) {
              if (e.tag === 13) e.memoizedState !== null && bh(e, n, t);
              else if (e.tag === 19) bh(e, n, t);
              else if (e.child !== null) {
                e.child.return = e, e = e.child;
                continue;
              }
              if (e === t) break e;
              for (; e.sibling === null; ) {
                if (e.return === null || e.return === t) break e;
                e = e.return;
              }
              e.sibling.return = e.return, e = e.sibling;
            }
            r &= 1;
          }
          switch (U(ft, r), s) {
            case "forwards":
              for (n = t.child, s = null; n !== null; ) e = n.alternate, e !== null && zl(e) === null && (s = n), n = n.sibling;
              n = s, n === null ? (s = t.child, t.child = null) : (s = n.sibling, n.sibling = null), ts(t, false, s, n, f);
              break;
            case "backwards":
              for (n = null, s = t.child, t.child = null; s !== null; ) {
                if (e = s.alternate, e !== null && zl(e) === null) {
                  t.child = s;
                  break;
                }
                e = s.sibling, s.sibling = n, n = s, s = e;
              }
              ts(t, true, n, null, f);
              break;
            case "together":
              ts(t, false, null, null, void 0);
              break;
            default:
              t.memoizedState = null;
          }
          return t.child;
        }
        function Ln(e, t, n) {
          if (e !== null && (t.dependencies = e.dependencies), oi |= t.lanes, (n & t.childLanes) === 0) if (e !== null) {
            if (tr(e, t, n, false), (n & t.childLanes) === 0) return null;
          } else return null;
          if (e !== null && t.child !== e.child) throw Error(i(153));
          if (t.child !== null) {
            for (e = t.child, n = Cn(e, e.pendingProps), t.child = n, n.return = t; e.sibling !== null; ) e = e.sibling, n = n.sibling = Cn(e, e.pendingProps), n.return = t;
            n.sibling = null;
          }
          return t.child;
        }
        function ns(e, t) {
          return (e.lanes & t) !== 0 ? true : (e = e.dependencies, !!(e !== null && vl(e)));
        }
        function xy(e, t, n) {
          switch (t.tag) {
            case 3:
              fe(t, t.stateNode.containerInfo), Pn(t, ct, e.memoizedState.cache), Ja();
              break;
            case 27:
            case 5:
              ke(t);
              break;
            case 4:
              fe(t, t.stateNode.containerInfo);
              break;
            case 10:
              Pn(t, t.type, t.memoizedProps.value);
              break;
            case 13:
              var r = t.memoizedState;
              if (r !== null) return r.dehydrated !== null ? (ti(t), t.flags |= 128, null) : (n & t.child.childLanes) !== 0 ? yh(e, t, n) : (ti(t), e = Ln(e, t, n), e !== null ? e.sibling : null);
              ti(t);
              break;
            case 19:
              var s = (e.flags & 128) !== 0;
              if (r = (n & t.childLanes) !== 0, r || (tr(e, t, n, false), r = (n & t.childLanes) !== 0), s) {
                if (r) return _h(e, t, n);
                t.flags |= 128;
              }
              if (s = t.memoizedState, s !== null && (s.rendering = null, s.tail = null, s.lastEffect = null), U(ft, ft.current), r) break;
              return null;
            case 22:
            case 23:
              return t.lanes = 0, hh(e, t, n);
            case 24:
              Pn(t, ct, e.memoizedState.cache);
          }
          return Ln(e, t, n);
        }
        function wh(e, t, n) {
          if (e !== null) if (e.memoizedProps !== t.pendingProps) yt = true;
          else {
            if (!ns(e, n) && (t.flags & 128) === 0) return yt = false, xy(e, t, n);
            yt = (e.flags & 131072) !== 0;
          }
          else yt = false, Be && (t.flags & 1048576) !== 0 && If(t, ml, t.index);
          switch (t.lanes = 0, t.tag) {
            case 16:
              e: {
                e = t.pendingProps;
                var r = t.elementType, s = r._init;
                if (r = s(r._payload), t.type = r, typeof r == "function") fu(r) ? (e = Mi(r, e), t.tag = 1, t = vh(null, t, r, e, n)) : (t.tag = 0, t = Ku(null, t, r, e, n));
                else {
                  if (r != null) {
                    if (s = r.$$typeof, s === G) {
                      t.tag = 11, t = ch(null, t, r, e, n);
                      break e;
                    } else if (s === T) {
                      t.tag = 14, t = fh(null, t, r, e, n);
                      break e;
                    }
                  }
                  throw t = ue(r) || r, Error(i(306, t, ""));
                }
              }
              return t;
            case 0:
              return Ku(e, t, t.type, t.pendingProps, n);
            case 1:
              return r = t.type, s = Mi(r, t.pendingProps), vh(e, t, r, s, n);
            case 3:
              e: {
                if (fe(t, t.stateNode.containerInfo), e === null) throw Error(i(387));
                r = t.pendingProps;
                var f = t.memoizedState;
                s = f.element, Tu(e, t), ur(t, r, null, n);
                var v = t.memoizedState;
                if (r = v.cache, Pn(t, ct, r), r !== f.cache && bu(t, [
                  ct
                ], n, true), or(), r = v.element, f.isDehydrated) if (f = {
                  element: r,
                  isDehydrated: false,
                  cache: v.cache
                }, t.updateQueue.baseState = f, t.memoizedState = f, t.flags & 256) {
                  t = ph(e, t, r, n);
                  break e;
                } else if (r !== s) {
                  s = Pt(Error(i(424)), t), er(s), t = ph(e, t, r, n);
                  break e;
                } else {
                  switch (e = t.stateNode.containerInfo, e.nodeType) {
                    case 9:
                      e = e.body;
                      break;
                    default:
                      e = e.nodeName === "HTML" ? e.ownerDocument.body : e;
                  }
                  for (it = dn(e.firstChild), Ot = t, Be = true, Ci = null, bn = true, n = Jd(t, null, r, n), t.child = n; n; ) n.flags = n.flags & -3 | 4096, n = n.sibling;
                }
                else {
                  if (Ja(), r === s) {
                    t = Ln(e, t, n);
                    break e;
                  }
                  Et(e, t, r, n);
                }
                t = t.child;
              }
              return t;
            case 26:
              return Ll(e, t), e === null ? (n = Tg(t.type, null, t.pendingProps, null)) ? t.memoizedState = n : Be || (n = t.type, e = t.pendingProps, r = Pl(ce.current).createElement(n), r[At] = t, r[zt] = e, xt(r, n, e), pt(r), t.stateNode = r) : t.memoizedState = Tg(t.type, e.memoizedProps, t.pendingProps, e.memoizedState), null;
            case 27:
              return ke(t), e === null && Be && (r = t.stateNode = Eg(t.type, t.pendingProps, ce.current), Ot = t, bn = true, s = it, fi(t.type) ? (Gs = s, it = dn(r.firstChild)) : it = s), Et(e, t, t.pendingProps.children, n), Ll(e, t), e === null && (t.flags |= 4194304), t.child;
            case 5:
              return e === null && Be && ((s = r = it) && (r = Wy(r, t.type, t.pendingProps, bn), r !== null ? (t.stateNode = r, Ot = t, it = dn(r.firstChild), bn = false, s = true) : s = false), s || Di(t)), ke(t), s = t.type, f = t.pendingProps, v = e !== null ? e.memoizedProps : null, r = f.children, zs(s, f) ? r = null : v !== null && zs(s, v) && (t.flags |= 32), t.memoizedState !== null && (s = Ou(e, t, vy, null, null, n), Nr._currentValue = s), Ll(e, t), Et(e, t, r, n), t.child;
            case 6:
              return e === null && Be && ((e = n = it) && (n = Jy(n, t.pendingProps, bn), n !== null ? (t.stateNode = n, Ot = t, it = null, e = true) : e = false), e || Di(t)), null;
            case 13:
              return yh(e, t, n);
            case 4:
              return fe(t, t.stateNode.containerInfo), r = t.pendingProps, e === null ? t.child = da(t, null, r, n) : Et(e, t, r, n), t.child;
            case 11:
              return ch(e, t, t.type, t.pendingProps, n);
            case 7:
              return Et(e, t, t.pendingProps, n), t.child;
            case 8:
              return Et(e, t, t.pendingProps.children, n), t.child;
            case 12:
              return Et(e, t, t.pendingProps.children, n), t.child;
            case 10:
              return r = t.pendingProps, Pn(t, t.type, r.value), Et(e, t, r.children, n), t.child;
            case 9:
              return s = t.type._context, r = t.pendingProps.children, Oi(t), s = Rt(s), r = r(s), t.flags |= 1, Et(e, t, r, n), t.child;
            case 14:
              return fh(e, t, t.type, t.pendingProps, n);
            case 15:
              return dh(e, t, t.type, t.pendingProps, n);
            case 19:
              return _h(e, t, n);
            case 31:
              return r = t.pendingProps, n = t.mode, r = {
                mode: r.mode,
                children: r.children
              }, e === null ? (n = Gl(r, n), n.ref = t.ref, t.child = n, n.return = t, t = n) : (n = Cn(e.child, r), n.ref = t.ref, t.child = n, n.return = t, t = n), t;
            case 22:
              return hh(e, t, n);
            case 24:
              return Oi(t), r = Rt(ct), e === null ? (s = Eu(), s === null && (s = Pe, f = _u(), s.pooledCache = f, f.refCount++, f !== null && (s.pooledCacheLanes |= n), s = f), t.memoizedState = {
                parent: r,
                cache: s
              }, xu(t), Pn(t, ct, s)) : ((e.lanes & n) !== 0 && (Tu(e, t), ur(t, null, null, n), or()), s = e.memoizedState, f = t.memoizedState, s.parent !== r ? (s = {
                parent: r,
                cache: r
              }, t.memoizedState = s, t.lanes === 0 && (t.memoizedState = t.updateQueue.baseState = s), Pn(t, ct, r)) : (r = f.cache, Pn(t, ct, r), r !== s.cache && bu(t, [
                ct
              ], n, true))), Et(e, t, t.pendingProps.children, n), t.child;
            case 29:
              throw t.pendingProps;
          }
          throw Error(i(156, t.tag));
        }
        function Gn(e) {
          e.flags |= 4;
        }
        function Eh(e, t) {
          if (t.type !== "stylesheet" || (t.state.loading & 4) !== 0) e.flags &= -16777217;
          else if (e.flags |= 16777216, !Ng(t)) {
            if (t = en.current, t !== null && ((Ge & 4194048) === Ge ? _n !== null : (Ge & 62914560) !== Ge && (Ge & 536870912) === 0 || t !== _n)) throw rr = Su, rd;
            e.flags |= 8192;
          }
        }
        function Ul(e, t) {
          t !== null && (e.flags |= 4), e.flags & 16384 && (t = e.tag !== 22 ? Jc() : 536870912, e.lanes |= t, va |= t);
        }
        function mr(e, t) {
          if (!Be) switch (e.tailMode) {
            case "hidden":
              t = e.tail;
              for (var n = null; t !== null; ) t.alternate !== null && (n = t), t = t.sibling;
              n === null ? e.tail = null : n.sibling = null;
              break;
            case "collapsed":
              n = e.tail;
              for (var r = null; n !== null; ) n.alternate !== null && (r = n), n = n.sibling;
              r === null ? t || e.tail === null ? e.tail = null : e.tail.sibling = null : r.sibling = null;
          }
        }
        function tt(e) {
          var t = e.alternate !== null && e.alternate.child === e.child, n = 0, r = 0;
          if (t) for (var s = e.child; s !== null; ) n |= s.lanes | s.childLanes, r |= s.subtreeFlags & 65011712, r |= s.flags & 65011712, s.return = e, s = s.sibling;
          else for (s = e.child; s !== null; ) n |= s.lanes | s.childLanes, r |= s.subtreeFlags, r |= s.flags, s.return = e, s = s.sibling;
          return e.subtreeFlags |= r, e.childLanes = n, t;
        }
        function Ty(e, t, n) {
          var r = t.pendingProps;
          switch (mu(t), t.tag) {
            case 31:
            case 16:
            case 15:
            case 0:
            case 11:
            case 7:
            case 8:
            case 12:
            case 9:
            case 14:
              return tt(t), null;
            case 1:
              return tt(t), null;
            case 3:
              return n = t.stateNode, r = null, e !== null && (r = e.memoizedState.cache), t.memoizedState.cache !== r && (t.flags |= 2048), zn(ct), _e(), n.pendingContext && (n.context = n.pendingContext, n.pendingContext = null), (e === null || e.child === null) && (Wa(t) ? Gn(t) : e === null || e.memoizedState.isDehydrated && (t.flags & 256) === 0 || (t.flags |= 1024, ed())), tt(t), null;
            case 26:
              return n = t.memoizedState, e === null ? (Gn(t), n !== null ? (tt(t), Eh(t, n)) : (tt(t), t.flags &= -16777217)) : n ? n !== e.memoizedState ? (Gn(t), tt(t), Eh(t, n)) : (tt(t), t.flags &= -16777217) : (e.memoizedProps !== r && Gn(t), tt(t), t.flags &= -16777217), null;
            case 27:
              pe(t), n = ce.current;
              var s = t.type;
              if (e !== null && t.stateNode != null) e.memoizedProps !== r && Gn(t);
              else {
                if (!r) {
                  if (t.stateNode === null) throw Error(i(166));
                  return tt(t), null;
                }
                e = ie.current, Wa(t) ? Wf(t) : (e = Eg(s, r, n), t.stateNode = e, Gn(t));
              }
              return tt(t), null;
            case 5:
              if (pe(t), n = t.type, e !== null && t.stateNode != null) e.memoizedProps !== r && Gn(t);
              else {
                if (!r) {
                  if (t.stateNode === null) throw Error(i(166));
                  return tt(t), null;
                }
                if (e = ie.current, Wa(t)) Wf(t);
                else {
                  switch (s = Pl(ce.current), e) {
                    case 1:
                      e = s.createElementNS("http://www.w3.org/2000/svg", n);
                      break;
                    case 2:
                      e = s.createElementNS("http://www.w3.org/1998/Math/MathML", n);
                      break;
                    default:
                      switch (n) {
                        case "svg":
                          e = s.createElementNS("http://www.w3.org/2000/svg", n);
                          break;
                        case "math":
                          e = s.createElementNS("http://www.w3.org/1998/Math/MathML", n);
                          break;
                        case "script":
                          e = s.createElement("div"), e.innerHTML = "<script><\/script>", e = e.removeChild(e.firstChild);
                          break;
                        case "select":
                          e = typeof r.is == "string" ? s.createElement("select", {
                            is: r.is
                          }) : s.createElement("select"), r.multiple ? e.multiple = true : r.size && (e.size = r.size);
                          break;
                        default:
                          e = typeof r.is == "string" ? s.createElement(n, {
                            is: r.is
                          }) : s.createElement(n);
                      }
                  }
                  e[At] = t, e[zt] = r;
                  e: for (s = t.child; s !== null; ) {
                    if (s.tag === 5 || s.tag === 6) e.appendChild(s.stateNode);
                    else if (s.tag !== 4 && s.tag !== 27 && s.child !== null) {
                      s.child.return = s, s = s.child;
                      continue;
                    }
                    if (s === t) break e;
                    for (; s.sibling === null; ) {
                      if (s.return === null || s.return === t) break e;
                      s = s.return;
                    }
                    s.sibling.return = s.return, s = s.sibling;
                  }
                  t.stateNode = e;
                  e: switch (xt(e, n, r), n) {
                    case "button":
                    case "input":
                    case "select":
                    case "textarea":
                      e = !!r.autoFocus;
                      break e;
                    case "img":
                      e = true;
                      break e;
                    default:
                      e = false;
                  }
                  e && Gn(t);
                }
              }
              return tt(t), t.flags &= -16777217, null;
            case 6:
              if (e && t.stateNode != null) e.memoizedProps !== r && Gn(t);
              else {
                if (typeof r != "string" && t.stateNode === null) throw Error(i(166));
                if (e = ce.current, Wa(t)) {
                  if (e = t.stateNode, n = t.memoizedProps, r = null, s = Ot, s !== null) switch (s.tag) {
                    case 27:
                    case 5:
                      r = s.memoizedProps;
                  }
                  e[At] = t, e = !!(e.nodeValue === n || r !== null && r.suppressHydrationWarning === true || mg(e.nodeValue, n)), e || Di(t);
                } else e = Pl(e).createTextNode(r), e[At] = t, t.stateNode = e;
              }
              return tt(t), null;
            case 13:
              if (r = t.memoizedState, e === null || e.memoizedState !== null && e.memoizedState.dehydrated !== null) {
                if (s = Wa(t), r !== null && r.dehydrated !== null) {
                  if (e === null) {
                    if (!s) throw Error(i(318));
                    if (s = t.memoizedState, s = s !== null ? s.dehydrated : null, !s) throw Error(i(317));
                    s[At] = t;
                  } else Ja(), (t.flags & 128) === 0 && (t.memoizedState = null), t.flags |= 4;
                  tt(t), s = false;
                } else s = ed(), e !== null && e.memoizedState !== null && (e.memoizedState.hydrationErrors = s), s = true;
                if (!s) return t.flags & 256 ? (Mn(t), t) : (Mn(t), null);
              }
              if (Mn(t), (t.flags & 128) !== 0) return t.lanes = n, t;
              if (n = r !== null, e = e !== null && e.memoizedState !== null, n) {
                r = t.child, s = null, r.alternate !== null && r.alternate.memoizedState !== null && r.alternate.memoizedState.cachePool !== null && (s = r.alternate.memoizedState.cachePool.pool);
                var f = null;
                r.memoizedState !== null && r.memoizedState.cachePool !== null && (f = r.memoizedState.cachePool.pool), f !== s && (r.flags |= 2048);
              }
              return n !== e && n && (t.child.flags |= 8192), Ul(t, t.updateQueue), tt(t), null;
            case 4:
              return _e(), e === null && Rs(t.stateNode.containerInfo), tt(t), null;
            case 10:
              return zn(t.type), tt(t), null;
            case 19:
              if (K(ft), s = t.memoizedState, s === null) return tt(t), null;
              if (r = (t.flags & 128) !== 0, f = s.rendering, f === null) if (r) mr(s, false);
              else {
                if (at !== 0 || e !== null && (e.flags & 128) !== 0) for (e = t.child; e !== null; ) {
                  if (f = zl(e), f !== null) {
                    for (t.flags |= 128, mr(s, false), e = f.updateQueue, t.updateQueue = e, Ul(t, e), t.subtreeFlags = 0, e = n, n = t.child; n !== null; ) Pf(n, e), n = n.sibling;
                    return U(ft, ft.current & 1 | 2), t.child;
                  }
                  e = e.sibling;
                }
                s.tail !== null && nt() > Hl && (t.flags |= 128, r = true, mr(s, false), t.lanes = 4194304);
              }
              else {
                if (!r) if (e = zl(f), e !== null) {
                  if (t.flags |= 128, r = true, e = e.updateQueue, t.updateQueue = e, Ul(t, e), mr(s, true), s.tail === null && s.tailMode === "hidden" && !f.alternate && !Be) return tt(t), null;
                } else 2 * nt() - s.renderingStartTime > Hl && n !== 536870912 && (t.flags |= 128, r = true, mr(s, false), t.lanes = 4194304);
                s.isBackwards ? (f.sibling = t.child, t.child = f) : (e = s.last, e !== null ? e.sibling = f : t.child = f, s.last = f);
              }
              return s.tail !== null ? (t = s.tail, s.rendering = t, s.tail = t.sibling, s.renderingStartTime = nt(), t.sibling = null, e = ft.current, U(ft, r ? e & 1 | 2 : e & 1), t) : (tt(t), null);
            case 22:
            case 23:
              return Mn(t), Du(), r = t.memoizedState !== null, e !== null ? e.memoizedState !== null !== r && (t.flags |= 8192) : r && (t.flags |= 8192), r ? (n & 536870912) !== 0 && (t.flags & 128) === 0 && (tt(t), t.subtreeFlags & 6 && (t.flags |= 8192)) : tt(t), n = t.updateQueue, n !== null && Ul(t, n.retryQueue), n = null, e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), r = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (r = t.memoizedState.cachePool.pool), r !== n && (t.flags |= 2048), e !== null && K(zi), null;
            case 24:
              return n = null, e !== null && (n = e.memoizedState.cache), t.memoizedState.cache !== n && (t.flags |= 2048), zn(ct), tt(t), null;
            case 25:
              return null;
            case 30:
              return null;
          }
          throw Error(i(156, t.tag));
        }
        function Ay(e, t) {
          switch (mu(t), t.tag) {
            case 1:
              return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
            case 3:
              return zn(ct), _e(), e = t.flags, (e & 65536) !== 0 && (e & 128) === 0 ? (t.flags = e & -65537 | 128, t) : null;
            case 26:
            case 27:
            case 5:
              return pe(t), null;
            case 13:
              if (Mn(t), e = t.memoizedState, e !== null && e.dehydrated !== null) {
                if (t.alternate === null) throw Error(i(340));
                Ja();
              }
              return e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
            case 19:
              return K(ft), null;
            case 4:
              return _e(), null;
            case 10:
              return zn(t.type), null;
            case 22:
            case 23:
              return Mn(t), Du(), e !== null && K(zi), e = t.flags, e & 65536 ? (t.flags = e & -65537 | 128, t) : null;
            case 24:
              return zn(ct), null;
            case 25:
              return null;
            default:
              return null;
          }
        }
        function Sh(e, t) {
          switch (mu(t), t.tag) {
            case 3:
              zn(ct), _e();
              break;
            case 26:
            case 27:
            case 5:
              pe(t);
              break;
            case 4:
              _e();
              break;
            case 13:
              Mn(t);
              break;
            case 19:
              K(ft);
              break;
            case 10:
              zn(t.type);
              break;
            case 22:
            case 23:
              Mn(t), Du(), e !== null && K(zi);
              break;
            case 24:
              zn(ct);
          }
        }
        function vr(e, t) {
          try {
            var n = t.updateQueue, r = n !== null ? n.lastEffect : null;
            if (r !== null) {
              var s = r.next;
              n = s;
              do {
                if ((n.tag & e) === e) {
                  r = void 0;
                  var f = n.create, v = n.inst;
                  r = f(), v.destroy = r;
                }
                n = n.next;
              } while (n !== s);
            }
          } catch (b) {
            Qe(t, t.return, b);
          }
        }
        function ii(e, t, n) {
          try {
            var r = t.updateQueue, s = r !== null ? r.lastEffect : null;
            if (s !== null) {
              var f = s.next;
              r = f;
              do {
                if ((r.tag & e) === e) {
                  var v = r.inst, b = v.destroy;
                  if (b !== void 0) {
                    v.destroy = void 0, s = t;
                    var A = n, j = b;
                    try {
                      j();
                    } catch (Q) {
                      Qe(s, A, Q);
                    }
                  }
                }
                r = r.next;
              } while (r !== f);
            }
          } catch (Q) {
            Qe(t, t.return, Q);
          }
        }
        function xh(e) {
          var t = e.updateQueue;
          if (t !== null) {
            var n = e.stateNode;
            try {
              fd(t, n);
            } catch (r) {
              Qe(e, e.return, r);
            }
          }
        }
        function Th(e, t, n) {
          n.props = Mi(e.type, e.memoizedProps), n.state = e.memoizedState;
          try {
            n.componentWillUnmount();
          } catch (r) {
            Qe(e, t, r);
          }
        }
        function pr(e, t) {
          try {
            var n = e.ref;
            if (n !== null) {
              switch (e.tag) {
                case 26:
                case 27:
                case 5:
                  var r = e.stateNode;
                  break;
                case 30:
                  r = e.stateNode;
                  break;
                default:
                  r = e.stateNode;
              }
              typeof n == "function" ? e.refCleanup = n(r) : n.current = r;
            }
          } catch (s) {
            Qe(e, t, s);
          }
        }
        function wn(e, t) {
          var n = e.ref, r = e.refCleanup;
          if (n !== null) if (typeof r == "function") try {
            r();
          } catch (s) {
            Qe(e, t, s);
          } finally {
            e.refCleanup = null, e = e.alternate, e != null && (e.refCleanup = null);
          }
          else if (typeof n == "function") try {
            n(null);
          } catch (s) {
            Qe(e, t, s);
          }
          else n.current = null;
        }
        function Ah(e) {
          var t = e.type, n = e.memoizedProps, r = e.stateNode;
          try {
            e: switch (t) {
              case "button":
              case "input":
              case "select":
              case "textarea":
                n.autoFocus && r.focus();
                break e;
              case "img":
                n.src ? r.src = n.src : n.srcSet && (r.srcset = n.srcSet);
            }
          } catch (s) {
            Qe(e, e.return, s);
          }
        }
        function is(e, t, n) {
          try {
            var r = e.stateNode;
            Zy(r, e.type, n, t), r[zt] = t;
          } catch (s) {
            Qe(e, e.return, s);
          }
        }
        function Rh(e) {
          return e.tag === 5 || e.tag === 3 || e.tag === 26 || e.tag === 27 && fi(e.type) || e.tag === 4;
        }
        function as(e) {
          e: for (; ; ) {
            for (; e.sibling === null; ) {
              if (e.return === null || Rh(e.return)) return null;
              e = e.return;
            }
            for (e.sibling.return = e.return, e = e.sibling; e.tag !== 5 && e.tag !== 6 && e.tag !== 18; ) {
              if (e.tag === 27 && fi(e.type) || e.flags & 2 || e.child === null || e.tag === 4) continue e;
              e.child.return = e, e = e.child;
            }
            if (!(e.flags & 2)) return e.stateNode;
          }
        }
        function rs(e, t, n) {
          var r = e.tag;
          if (r === 5 || r === 6) e = e.stateNode, t ? (n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n).insertBefore(e, t) : (t = n.nodeType === 9 ? n.body : n.nodeName === "HTML" ? n.ownerDocument.body : n, t.appendChild(e), n = n._reactRootContainer, n != null || t.onclick !== null || (t.onclick = Kl));
          else if (r !== 4 && (r === 27 && fi(e.type) && (n = e.stateNode, t = null), e = e.child, e !== null)) for (rs(e, t, n), e = e.sibling; e !== null; ) rs(e, t, n), e = e.sibling;
        }
        function jl(e, t, n) {
          var r = e.tag;
          if (r === 5 || r === 6) e = e.stateNode, t ? n.insertBefore(e, t) : n.appendChild(e);
          else if (r !== 4 && (r === 27 && fi(e.type) && (n = e.stateNode), e = e.child, e !== null)) for (jl(e, t, n), e = e.sibling; e !== null; ) jl(e, t, n), e = e.sibling;
        }
        function Ch(e) {
          var t = e.stateNode, n = e.memoizedProps;
          try {
            for (var r = e.type, s = t.attributes; s.length; ) t.removeAttributeNode(s[0]);
            xt(t, r, n), t[At] = e, t[zt] = n;
          } catch (f) {
            Qe(e, e.return, f);
          }
        }
        var Un = false, lt = false, ls = false, Dh = typeof WeakSet == "function" ? WeakSet : Set, bt = null;
        function Ry(e, t) {
          if (e = e.containerInfo, Ns = no, e = Hf(e), au(e)) {
            if ("selectionStart" in e) var n = {
              start: e.selectionStart,
              end: e.selectionEnd
            };
            else e: {
              n = (n = e.ownerDocument) && n.defaultView || window;
              var r = n.getSelection && n.getSelection();
              if (r && r.rangeCount !== 0) {
                n = r.anchorNode;
                var s = r.anchorOffset, f = r.focusNode;
                r = r.focusOffset;
                try {
                  n.nodeType, f.nodeType;
                } catch {
                  n = null;
                  break e;
                }
                var v = 0, b = -1, A = -1, j = 0, Q = 0, W = e, F = null;
                t: for (; ; ) {
                  for (var q; W !== n || s !== 0 && W.nodeType !== 3 || (b = v + s), W !== f || r !== 0 && W.nodeType !== 3 || (A = v + r), W.nodeType === 3 && (v += W.nodeValue.length), (q = W.firstChild) !== null; ) F = W, W = q;
                  for (; ; ) {
                    if (W === e) break t;
                    if (F === n && ++j === s && (b = v), F === f && ++Q === r && (A = v), (q = W.nextSibling) !== null) break;
                    W = F, F = W.parentNode;
                  }
                  W = q;
                }
                n = b === -1 || A === -1 ? null : {
                  start: b,
                  end: A
                };
              } else n = null;
            }
            n = n || {
              start: 0,
              end: 0
            };
          } else n = null;
          for (Os = {
            focusedElem: e,
            selectionRange: n
          }, no = false, bt = t; bt !== null; ) if (t = bt, e = t.child, (t.subtreeFlags & 1024) !== 0 && e !== null) e.return = t, bt = e;
          else for (; bt !== null; ) {
            switch (t = bt, f = t.alternate, e = t.flags, t.tag) {
              case 0:
                break;
              case 11:
              case 15:
                break;
              case 1:
                if ((e & 1024) !== 0 && f !== null) {
                  e = void 0, n = t, s = f.memoizedProps, f = f.memoizedState, r = n.stateNode;
                  try {
                    var xe = Mi(n.type, s, n.elementType === n.type);
                    e = r.getSnapshotBeforeUpdate(xe, f), r.__reactInternalSnapshotBeforeUpdate = e;
                  } catch (we) {
                    Qe(n, n.return, we);
                  }
                }
                break;
              case 3:
                if ((e & 1024) !== 0) {
                  if (e = t.stateNode.containerInfo, n = e.nodeType, n === 9) Ms(e);
                  else if (n === 1) switch (e.nodeName) {
                    case "HEAD":
                    case "HTML":
                    case "BODY":
                      Ms(e);
                      break;
                    default:
                      e.textContent = "";
                  }
                }
                break;
              case 5:
              case 26:
              case 27:
              case 6:
              case 4:
              case 17:
                break;
              default:
                if ((e & 1024) !== 0) throw Error(i(163));
            }
            if (e = t.sibling, e !== null) {
              e.return = t.return, bt = e;
              break;
            }
            bt = t.return;
          }
        }
        function Nh(e, t, n) {
          var r = n.flags;
          switch (n.tag) {
            case 0:
            case 11:
            case 15:
              ai(e, n), r & 4 && vr(5, n);
              break;
            case 1:
              if (ai(e, n), r & 4) if (e = n.stateNode, t === null) try {
                e.componentDidMount();
              } catch (v) {
                Qe(n, n.return, v);
              }
              else {
                var s = Mi(n.type, t.memoizedProps);
                t = t.memoizedState;
                try {
                  e.componentDidUpdate(s, t, e.__reactInternalSnapshotBeforeUpdate);
                } catch (v) {
                  Qe(n, n.return, v);
                }
              }
              r & 64 && xh(n), r & 512 && pr(n, n.return);
              break;
            case 3:
              if (ai(e, n), r & 64 && (e = n.updateQueue, e !== null)) {
                if (t = null, n.child !== null) switch (n.child.tag) {
                  case 27:
                  case 5:
                    t = n.child.stateNode;
                    break;
                  case 1:
                    t = n.child.stateNode;
                }
                try {
                  fd(e, t);
                } catch (v) {
                  Qe(n, n.return, v);
                }
              }
              break;
            case 27:
              t === null && r & 4 && Ch(n);
            case 26:
            case 5:
              ai(e, n), t === null && r & 4 && Ah(n), r & 512 && pr(n, n.return);
              break;
            case 12:
              ai(e, n);
              break;
            case 13:
              ai(e, n), r & 4 && kh(e, n), r & 64 && (e = n.memoizedState, e !== null && (e = e.dehydrated, e !== null && (n = Gy.bind(null, n), eb(e, n))));
              break;
            case 22:
              if (r = n.memoizedState !== null || Un, !r) {
                t = t !== null && t.memoizedState !== null || lt, s = Un;
                var f = lt;
                Un = r, (lt = t) && !f ? ri(e, n, (n.subtreeFlags & 8772) !== 0) : ai(e, n), Un = s, lt = f;
              }
              break;
            case 30:
              break;
            default:
              ai(e, n);
          }
        }
        function Oh(e) {
          var t = e.alternate;
          t !== null && (e.alternate = null, Oh(t)), e.child = null, e.deletions = null, e.sibling = null, e.tag === 5 && (t = e.stateNode, t !== null && Bo(t)), e.stateNode = null, e.return = null, e.dependencies = null, e.memoizedProps = null, e.memoizedState = null, e.pendingProps = null, e.stateNode = null, e.updateQueue = null;
        }
        var et = null, Lt = false;
        function jn(e, t, n) {
          for (n = n.child; n !== null; ) zh(e, t, n), n = n.sibling;
        }
        function zh(e, t, n) {
          if (Ve && typeof Ve.onCommitFiberUnmount == "function") try {
            Ve.onCommitFiberUnmount(Je, n);
          } catch {
          }
          switch (n.tag) {
            case 26:
              lt || wn(n, t), jn(e, t, n), n.memoizedState ? n.memoizedState.count-- : n.stateNode && (n = n.stateNode, n.parentNode.removeChild(n));
              break;
            case 27:
              lt || wn(n, t);
              var r = et, s = Lt;
              fi(n.type) && (et = n.stateNode, Lt = false), jn(e, t, n), Ar(n.stateNode), et = r, Lt = s;
              break;
            case 5:
              lt || wn(n, t);
            case 6:
              if (r = et, s = Lt, et = null, jn(e, t, n), et = r, Lt = s, et !== null) if (Lt) try {
                (et.nodeType === 9 ? et.body : et.nodeName === "HTML" ? et.ownerDocument.body : et).removeChild(n.stateNode);
              } catch (f) {
                Qe(n, t, f);
              }
              else try {
                et.removeChild(n.stateNode);
              } catch (f) {
                Qe(n, t, f);
              }
              break;
            case 18:
              et !== null && (Lt ? (e = et, _g(e.nodeType === 9 ? e.body : e.nodeName === "HTML" ? e.ownerDocument.body : e, n.stateNode), Mr(e)) : _g(et, n.stateNode));
              break;
            case 4:
              r = et, s = Lt, et = n.stateNode.containerInfo, Lt = true, jn(e, t, n), et = r, Lt = s;
              break;
            case 0:
            case 11:
            case 14:
            case 15:
              lt || ii(2, n, t), lt || ii(4, n, t), jn(e, t, n);
              break;
            case 1:
              lt || (wn(n, t), r = n.stateNode, typeof r.componentWillUnmount == "function" && Th(n, t, r)), jn(e, t, n);
              break;
            case 21:
              jn(e, t, n);
              break;
            case 22:
              lt = (r = lt) || n.memoizedState !== null, jn(e, t, n), lt = r;
              break;
            default:
              jn(e, t, n);
          }
        }
        function kh(e, t) {
          if (t.memoizedState === null && (e = t.alternate, e !== null && (e = e.memoizedState, e !== null && (e = e.dehydrated, e !== null)))) try {
            Mr(e);
          } catch (n) {
            Qe(t, t.return, n);
          }
        }
        function Cy(e) {
          switch (e.tag) {
            case 13:
            case 19:
              var t = e.stateNode;
              return t === null && (t = e.stateNode = new Dh()), t;
            case 22:
              return e = e.stateNode, t = e._retryCache, t === null && (t = e._retryCache = new Dh()), t;
            default:
              throw Error(i(435, e.tag));
          }
        }
        function os(e, t) {
          var n = Cy(e);
          t.forEach(function(r) {
            var s = Uy.bind(null, e, r);
            n.has(r) || (n.add(r), r.then(s, s));
          });
        }
        function Bt(e, t) {
          var n = t.deletions;
          if (n !== null) for (var r = 0; r < n.length; r++) {
            var s = n[r], f = e, v = t, b = v;
            e: for (; b !== null; ) {
              switch (b.tag) {
                case 27:
                  if (fi(b.type)) {
                    et = b.stateNode, Lt = false;
                    break e;
                  }
                  break;
                case 5:
                  et = b.stateNode, Lt = false;
                  break e;
                case 3:
                case 4:
                  et = b.stateNode.containerInfo, Lt = true;
                  break e;
              }
              b = b.return;
            }
            if (et === null) throw Error(i(160));
            zh(f, v, s), et = null, Lt = false, f = s.alternate, f !== null && (f.return = null), s.return = null;
          }
          if (t.subtreeFlags & 13878) for (t = t.child; t !== null; ) Mh(t, e), t = t.sibling;
        }
        var fn = null;
        function Mh(e, t) {
          var n = e.alternate, r = e.flags;
          switch (e.tag) {
            case 0:
            case 11:
            case 14:
            case 15:
              Bt(t, e), Ht(e), r & 4 && (ii(3, e, e.return), vr(3, e), ii(5, e, e.return));
              break;
            case 1:
              Bt(t, e), Ht(e), r & 512 && (lt || n === null || wn(n, n.return)), r & 64 && Un && (e = e.updateQueue, e !== null && (r = e.callbacks, r !== null && (n = e.shared.hiddenCallbacks, e.shared.hiddenCallbacks = n === null ? r : n.concat(r))));
              break;
            case 26:
              var s = fn;
              if (Bt(t, e), Ht(e), r & 512 && (lt || n === null || wn(n, n.return)), r & 4) {
                var f = n !== null ? n.memoizedState : null;
                if (r = e.memoizedState, n === null) if (r === null) if (e.stateNode === null) {
                  e: {
                    r = e.type, n = e.memoizedProps, s = s.ownerDocument || s;
                    t: switch (r) {
                      case "title":
                        f = s.getElementsByTagName("title")[0], (!f || f[Fa] || f[At] || f.namespaceURI === "http://www.w3.org/2000/svg" || f.hasAttribute("itemprop")) && (f = s.createElement(r), s.head.insertBefore(f, s.querySelector("head > title"))), xt(f, r, n), f[At] = e, pt(f), r = f;
                        break e;
                      case "link":
                        var v = Cg("link", "href", s).get(r + (n.href || ""));
                        if (v) {
                          for (var b = 0; b < v.length; b++) if (f = v[b], f.getAttribute("href") === (n.href == null || n.href === "" ? null : n.href) && f.getAttribute("rel") === (n.rel == null ? null : n.rel) && f.getAttribute("title") === (n.title == null ? null : n.title) && f.getAttribute("crossorigin") === (n.crossOrigin == null ? null : n.crossOrigin)) {
                            v.splice(b, 1);
                            break t;
                          }
                        }
                        f = s.createElement(r), xt(f, r, n), s.head.appendChild(f);
                        break;
                      case "meta":
                        if (v = Cg("meta", "content", s).get(r + (n.content || ""))) {
                          for (b = 0; b < v.length; b++) if (f = v[b], f.getAttribute("content") === (n.content == null ? null : "" + n.content) && f.getAttribute("name") === (n.name == null ? null : n.name) && f.getAttribute("property") === (n.property == null ? null : n.property) && f.getAttribute("http-equiv") === (n.httpEquiv == null ? null : n.httpEquiv) && f.getAttribute("charset") === (n.charSet == null ? null : n.charSet)) {
                            v.splice(b, 1);
                            break t;
                          }
                        }
                        f = s.createElement(r), xt(f, r, n), s.head.appendChild(f);
                        break;
                      default:
                        throw Error(i(468, r));
                    }
                    f[At] = e, pt(f), r = f;
                  }
                  e.stateNode = r;
                } else Dg(s, e.type, e.stateNode);
                else e.stateNode = Rg(s, r, e.memoizedProps);
                else f !== r ? (f === null ? n.stateNode !== null && (n = n.stateNode, n.parentNode.removeChild(n)) : f.count--, r === null ? Dg(s, e.type, e.stateNode) : Rg(s, r, e.memoizedProps)) : r === null && e.stateNode !== null && is(e, e.memoizedProps, n.memoizedProps);
              }
              break;
            case 27:
              Bt(t, e), Ht(e), r & 512 && (lt || n === null || wn(n, n.return)), n !== null && r & 4 && is(e, e.memoizedProps, n.memoizedProps);
              break;
            case 5:
              if (Bt(t, e), Ht(e), r & 512 && (lt || n === null || wn(n, n.return)), e.flags & 32) {
                s = e.stateNode;
                try {
                  Qi(s, "");
                } catch (q) {
                  Qe(e, e.return, q);
                }
              }
              r & 4 && e.stateNode != null && (s = e.memoizedProps, is(e, s, n !== null ? n.memoizedProps : s)), r & 1024 && (ls = true);
              break;
            case 6:
              if (Bt(t, e), Ht(e), r & 4) {
                if (e.stateNode === null) throw Error(i(162));
                r = e.memoizedProps, n = e.stateNode;
                try {
                  n.nodeValue = r;
                } catch (q) {
                  Qe(e, e.return, q);
                }
              }
              break;
            case 3:
              if (Jl = null, s = fn, fn = Il(t.containerInfo), Bt(t, e), fn = s, Ht(e), r & 4 && n !== null && n.memoizedState.isDehydrated) try {
                Mr(t.containerInfo);
              } catch (q) {
                Qe(e, e.return, q);
              }
              ls && (ls = false, Lh(e));
              break;
            case 4:
              r = fn, fn = Il(e.stateNode.containerInfo), Bt(t, e), Ht(e), fn = r;
              break;
            case 12:
              Bt(t, e), Ht(e);
              break;
            case 13:
              Bt(t, e), Ht(e), e.child.flags & 8192 && e.memoizedState !== null != (n !== null && n.memoizedState !== null) && (hs = nt()), r & 4 && (r = e.updateQueue, r !== null && (e.updateQueue = null, os(e, r)));
              break;
            case 22:
              s = e.memoizedState !== null;
              var A = n !== null && n.memoizedState !== null, j = Un, Q = lt;
              if (Un = j || s, lt = Q || A, Bt(t, e), lt = Q, Un = j, Ht(e), r & 8192) e: for (t = e.stateNode, t._visibility = s ? t._visibility & -2 : t._visibility | 1, s && (n === null || A || Un || lt || Li(e)), n = null, t = e; ; ) {
                if (t.tag === 5 || t.tag === 26) {
                  if (n === null) {
                    A = n = t;
                    try {
                      if (f = A.stateNode, s) v = f.style, typeof v.setProperty == "function" ? v.setProperty("display", "none", "important") : v.display = "none";
                      else {
                        b = A.stateNode;
                        var W = A.memoizedProps.style, F = W != null && W.hasOwnProperty("display") ? W.display : null;
                        b.style.display = F == null || typeof F == "boolean" ? "" : ("" + F).trim();
                      }
                    } catch (q) {
                      Qe(A, A.return, q);
                    }
                  }
                } else if (t.tag === 6) {
                  if (n === null) {
                    A = t;
                    try {
                      A.stateNode.nodeValue = s ? "" : A.memoizedProps;
                    } catch (q) {
                      Qe(A, A.return, q);
                    }
                  }
                } else if ((t.tag !== 22 && t.tag !== 23 || t.memoizedState === null || t === e) && t.child !== null) {
                  t.child.return = t, t = t.child;
                  continue;
                }
                if (t === e) break e;
                for (; t.sibling === null; ) {
                  if (t.return === null || t.return === e) break e;
                  n === t && (n = null), t = t.return;
                }
                n === t && (n = null), t.sibling.return = t.return, t = t.sibling;
              }
              r & 4 && (r = e.updateQueue, r !== null && (n = r.retryQueue, n !== null && (r.retryQueue = null, os(e, n))));
              break;
            case 19:
              Bt(t, e), Ht(e), r & 4 && (r = e.updateQueue, r !== null && (e.updateQueue = null, os(e, r)));
              break;
            case 30:
              break;
            case 21:
              break;
            default:
              Bt(t, e), Ht(e);
          }
        }
        function Ht(e) {
          var t = e.flags;
          if (t & 2) {
            try {
              for (var n, r = e.return; r !== null; ) {
                if (Rh(r)) {
                  n = r;
                  break;
                }
                r = r.return;
              }
              if (n == null) throw Error(i(160));
              switch (n.tag) {
                case 27:
                  var s = n.stateNode, f = as(e);
                  jl(e, f, s);
                  break;
                case 5:
                  var v = n.stateNode;
                  n.flags & 32 && (Qi(v, ""), n.flags &= -33);
                  var b = as(e);
                  jl(e, b, v);
                  break;
                case 3:
                case 4:
                  var A = n.stateNode.containerInfo, j = as(e);
                  rs(e, j, A);
                  break;
                default:
                  throw Error(i(161));
              }
            } catch (Q) {
              Qe(e, e.return, Q);
            }
            e.flags &= -3;
          }
          t & 4096 && (e.flags &= -4097);
        }
        function Lh(e) {
          if (e.subtreeFlags & 1024) for (e = e.child; e !== null; ) {
            var t = e;
            Lh(t), t.tag === 5 && t.flags & 1024 && t.stateNode.reset(), e = e.sibling;
          }
        }
        function ai(e, t) {
          if (t.subtreeFlags & 8772) for (t = t.child; t !== null; ) Nh(e, t.alternate, t), t = t.sibling;
        }
        function Li(e) {
          for (e = e.child; e !== null; ) {
            var t = e;
            switch (t.tag) {
              case 0:
              case 11:
              case 14:
              case 15:
                ii(4, t, t.return), Li(t);
                break;
              case 1:
                wn(t, t.return);
                var n = t.stateNode;
                typeof n.componentWillUnmount == "function" && Th(t, t.return, n), Li(t);
                break;
              case 27:
                Ar(t.stateNode);
              case 26:
              case 5:
                wn(t, t.return), Li(t);
                break;
              case 22:
                t.memoizedState === null && Li(t);
                break;
              case 30:
                Li(t);
                break;
              default:
                Li(t);
            }
            e = e.sibling;
          }
        }
        function ri(e, t, n) {
          for (n = n && (t.subtreeFlags & 8772) !== 0, t = t.child; t !== null; ) {
            var r = t.alternate, s = e, f = t, v = f.flags;
            switch (f.tag) {
              case 0:
              case 11:
              case 15:
                ri(s, f, n), vr(4, f);
                break;
              case 1:
                if (ri(s, f, n), r = f, s = r.stateNode, typeof s.componentDidMount == "function") try {
                  s.componentDidMount();
                } catch (j) {
                  Qe(r, r.return, j);
                }
                if (r = f, s = r.updateQueue, s !== null) {
                  var b = r.stateNode;
                  try {
                    var A = s.shared.hiddenCallbacks;
                    if (A !== null) for (s.shared.hiddenCallbacks = null, s = 0; s < A.length; s++) cd(A[s], b);
                  } catch (j) {
                    Qe(r, r.return, j);
                  }
                }
                n && v & 64 && xh(f), pr(f, f.return);
                break;
              case 27:
                Ch(f);
              case 26:
              case 5:
                ri(s, f, n), n && r === null && v & 4 && Ah(f), pr(f, f.return);
                break;
              case 12:
                ri(s, f, n);
                break;
              case 13:
                ri(s, f, n), n && v & 4 && kh(s, f);
                break;
              case 22:
                f.memoizedState === null && ri(s, f, n), pr(f, f.return);
                break;
              case 30:
                break;
              default:
                ri(s, f, n);
            }
            t = t.sibling;
          }
        }
        function us(e, t) {
          var n = null;
          e !== null && e.memoizedState !== null && e.memoizedState.cachePool !== null && (n = e.memoizedState.cachePool.pool), e = null, t.memoizedState !== null && t.memoizedState.cachePool !== null && (e = t.memoizedState.cachePool.pool), e !== n && (e != null && e.refCount++, n != null && nr(n));
        }
        function ss(e, t) {
          e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && nr(e));
        }
        function En(e, t, n, r) {
          if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) Gh(e, t, n, r), t = t.sibling;
        }
        function Gh(e, t, n, r) {
          var s = t.flags;
          switch (t.tag) {
            case 0:
            case 11:
            case 15:
              En(e, t, n, r), s & 2048 && vr(9, t);
              break;
            case 1:
              En(e, t, n, r);
              break;
            case 3:
              En(e, t, n, r), s & 2048 && (e = null, t.alternate !== null && (e = t.alternate.memoizedState.cache), t = t.memoizedState.cache, t !== e && (t.refCount++, e != null && nr(e)));
              break;
            case 12:
              if (s & 2048) {
                En(e, t, n, r), e = t.stateNode;
                try {
                  var f = t.memoizedProps, v = f.id, b = f.onPostCommit;
                  typeof b == "function" && b(v, t.alternate === null ? "mount" : "update", e.passiveEffectDuration, -0);
                } catch (A) {
                  Qe(t, t.return, A);
                }
              } else En(e, t, n, r);
              break;
            case 13:
              En(e, t, n, r);
              break;
            case 23:
              break;
            case 22:
              f = t.stateNode, v = t.alternate, t.memoizedState !== null ? f._visibility & 2 ? En(e, t, n, r) : yr(e, t) : f._visibility & 2 ? En(e, t, n, r) : (f._visibility |= 2, ha(e, t, n, r, (t.subtreeFlags & 10256) !== 0)), s & 2048 && us(v, t);
              break;
            case 24:
              En(e, t, n, r), s & 2048 && ss(t.alternate, t);
              break;
            default:
              En(e, t, n, r);
          }
        }
        function ha(e, t, n, r, s) {
          for (s = s && (t.subtreeFlags & 10256) !== 0, t = t.child; t !== null; ) {
            var f = e, v = t, b = n, A = r, j = v.flags;
            switch (v.tag) {
              case 0:
              case 11:
              case 15:
                ha(f, v, b, A, s), vr(8, v);
                break;
              case 23:
                break;
              case 22:
                var Q = v.stateNode;
                v.memoizedState !== null ? Q._visibility & 2 ? ha(f, v, b, A, s) : yr(f, v) : (Q._visibility |= 2, ha(f, v, b, A, s)), s && j & 2048 && us(v.alternate, v);
                break;
              case 24:
                ha(f, v, b, A, s), s && j & 2048 && ss(v.alternate, v);
                break;
              default:
                ha(f, v, b, A, s);
            }
            t = t.sibling;
          }
        }
        function yr(e, t) {
          if (t.subtreeFlags & 10256) for (t = t.child; t !== null; ) {
            var n = e, r = t, s = r.flags;
            switch (r.tag) {
              case 22:
                yr(n, r), s & 2048 && us(r.alternate, r);
                break;
              case 24:
                yr(n, r), s & 2048 && ss(r.alternate, r);
                break;
              default:
                yr(n, r);
            }
            t = t.sibling;
          }
        }
        var br = 8192;
        function ga(e) {
          if (e.subtreeFlags & br) for (e = e.child; e !== null; ) Uh(e), e = e.sibling;
        }
        function Uh(e) {
          switch (e.tag) {
            case 26:
              ga(e), e.flags & br && e.memoizedState !== null && hb(fn, e.memoizedState, e.memoizedProps);
              break;
            case 5:
              ga(e);
              break;
            case 3:
            case 4:
              var t = fn;
              fn = Il(e.stateNode.containerInfo), ga(e), fn = t;
              break;
            case 22:
              e.memoizedState === null && (t = e.alternate, t !== null && t.memoizedState !== null ? (t = br, br = 16777216, ga(e), br = t) : ga(e));
              break;
            default:
              ga(e);
          }
        }
        function jh(e) {
          var t = e.alternate;
          if (t !== null && (e = t.child, e !== null)) {
            t.child = null;
            do
              t = e.sibling, e.sibling = null, e = t;
            while (e !== null);
          }
        }
        function _r(e) {
          var t = e.deletions;
          if ((e.flags & 16) !== 0) {
            if (t !== null) for (var n = 0; n < t.length; n++) {
              var r = t[n];
              bt = r, Hh(r, e);
            }
            jh(e);
          }
          if (e.subtreeFlags & 10256) for (e = e.child; e !== null; ) Bh(e), e = e.sibling;
        }
        function Bh(e) {
          switch (e.tag) {
            case 0:
            case 11:
            case 15:
              _r(e), e.flags & 2048 && ii(9, e, e.return);
              break;
            case 3:
              _r(e);
              break;
            case 12:
              _r(e);
              break;
            case 22:
              var t = e.stateNode;
              e.memoizedState !== null && t._visibility & 2 && (e.return === null || e.return.tag !== 13) ? (t._visibility &= -3, Bl(e)) : _r(e);
              break;
            default:
              _r(e);
          }
        }
        function Bl(e) {
          var t = e.deletions;
          if ((e.flags & 16) !== 0) {
            if (t !== null) for (var n = 0; n < t.length; n++) {
              var r = t[n];
              bt = r, Hh(r, e);
            }
            jh(e);
          }
          for (e = e.child; e !== null; ) {
            switch (t = e, t.tag) {
              case 0:
              case 11:
              case 15:
                ii(8, t, t.return), Bl(t);
                break;
              case 22:
                n = t.stateNode, n._visibility & 2 && (n._visibility &= -3, Bl(t));
                break;
              default:
                Bl(t);
            }
            e = e.sibling;
          }
        }
        function Hh(e, t) {
          for (; bt !== null; ) {
            var n = bt;
            switch (n.tag) {
              case 0:
              case 11:
              case 15:
                ii(8, n, t);
                break;
              case 23:
              case 22:
                if (n.memoizedState !== null && n.memoizedState.cachePool !== null) {
                  var r = n.memoizedState.cachePool.pool;
                  r != null && r.refCount++;
                }
                break;
              case 24:
                nr(n.memoizedState.cache);
            }
            if (r = n.child, r !== null) r.return = n, bt = r;
            else e: for (n = e; bt !== null; ) {
              r = bt;
              var s = r.sibling, f = r.return;
              if (Oh(r), r === n) {
                bt = null;
                break e;
              }
              if (s !== null) {
                s.return = f, bt = s;
                break e;
              }
              bt = f;
            }
          }
        }
        var Dy = {
          getCacheForType: function(e) {
            var t = Rt(ct), n = t.data.get(e);
            return n === void 0 && (n = e(), t.data.set(e, n)), n;
          }
        }, Ny = typeof WeakMap == "function" ? WeakMap : Map, He = 0, Pe = null, Oe = null, Ge = 0, Fe = 0, Ft = null, li = false, ma = false, cs = false, Bn = 0, at = 0, oi = 0, Gi = 0, fs = 0, tn = 0, va = 0, wr = null, Gt = null, ds = false, hs = 0, Hl = 1 / 0, Fl = null, ui = null, St = 0, si = null, pa = null, ya = 0, gs = 0, ms = null, Fh = null, Er = 0, vs = null;
        function qt() {
          if ((He & 2) !== 0 && Ge !== 0) return Ge & -Ge;
          if (D.T !== null) {
            var e = ra;
            return e !== 0 ? e : Ss();
          }
          return nf();
        }
        function qh() {
          tn === 0 && (tn = (Ge & 536870912) === 0 || Be ? Wc() : 536870912);
          var e = en.current;
          return e !== null && (e.flags |= 32), tn;
        }
        function Vt(e, t, n) {
          (e === Pe && (Fe === 2 || Fe === 9) || e.cancelPendingCommit !== null) && (ba(e, 0), ci(e, Ge, tn, false)), Ha(e, n), ((He & 2) === 0 || e !== Pe) && (e === Pe && ((He & 2) === 0 && (Gi |= n), at === 4 && ci(e, Ge, tn, false)), Sn(e));
        }
        function Vh(e, t, n) {
          if ((He & 6) !== 0) throw Error(i(327));
          var r = !n && (t & 124) === 0 && (t & e.expiredLanes) === 0 || Ba(e, t), s = r ? ky(e, t) : bs(e, t, true), f = r;
          do {
            if (s === 0) {
              ma && !r && ci(e, t, 0, false);
              break;
            } else {
              if (n = e.current.alternate, f && !Oy(n)) {
                s = bs(e, t, false), f = false;
                continue;
              }
              if (s === 2) {
                if (f = t, e.errorRecoveryDisabledLanes & f) var v = 0;
                else v = e.pendingLanes & -536870913, v = v !== 0 ? v : v & 536870912 ? 536870912 : 0;
                if (v !== 0) {
                  t = v;
                  e: {
                    var b = e;
                    s = wr;
                    var A = b.current.memoizedState.isDehydrated;
                    if (A && (ba(b, v).flags |= 256), v = bs(b, v, false), v !== 2) {
                      if (cs && !A) {
                        b.errorRecoveryDisabledLanes |= f, Gi |= f, s = 4;
                        break e;
                      }
                      f = Gt, Gt = s, f !== null && (Gt === null ? Gt = f : Gt.push.apply(Gt, f));
                    }
                    s = v;
                  }
                  if (f = false, s !== 2) continue;
                }
              }
              if (s === 1) {
                ba(e, 0), ci(e, t, 0, true);
                break;
              }
              e: {
                switch (r = e, f = s, f) {
                  case 0:
                  case 1:
                    throw Error(i(345));
                  case 4:
                    if ((t & 4194048) !== t) break;
                  case 6:
                    ci(r, t, tn, !li);
                    break e;
                  case 2:
                    Gt = null;
                    break;
                  case 3:
                  case 5:
                    break;
                  default:
                    throw Error(i(329));
                }
                if ((t & 62914560) === t && (s = hs + 300 - nt(), 10 < s)) {
                  if (ci(r, t, tn, !li), Wr(r, 0, true) !== 0) break e;
                  r.timeoutHandle = yg(Yh.bind(null, r, n, Gt, Fl, ds, t, tn, Gi, va, li, f, 2, -0, 0), s);
                  break e;
                }
                Yh(r, n, Gt, Fl, ds, t, tn, Gi, va, li, f, 0, -0, 0);
              }
            }
            break;
          } while (true);
          Sn(e);
        }
        function Yh(e, t, n, r, s, f, v, b, A, j, Q, W, F, q) {
          if (e.timeoutHandle = -1, W = t.subtreeFlags, (W & 8192 || (W & 16785408) === 16785408) && (Dr = {
            stylesheets: null,
            count: 0,
            unsuspend: db
          }, Uh(t), W = gb(), W !== null)) {
            e.cancelPendingCommit = W(Ih.bind(null, e, t, f, n, r, s, v, b, A, Q, 1, F, q)), ci(e, f, v, !j);
            return;
          }
          Ih(e, t, f, n, r, s, v, b, A);
        }
        function Oy(e) {
          for (var t = e; ; ) {
            var n = t.tag;
            if ((n === 0 || n === 11 || n === 15) && t.flags & 16384 && (n = t.updateQueue, n !== null && (n = n.stores, n !== null))) for (var r = 0; r < n.length; r++) {
              var s = n[r], f = s.getSnapshot;
              s = s.value;
              try {
                if (!Ut(f(), s)) return false;
              } catch {
                return false;
              }
            }
            if (n = t.child, t.subtreeFlags & 16384 && n !== null) n.return = t, t = n;
            else {
              if (t === e) break;
              for (; t.sibling === null; ) {
                if (t.return === null || t.return === e) return true;
                t = t.return;
              }
              t.sibling.return = t.return, t = t.sibling;
            }
          }
          return true;
        }
        function ci(e, t, n, r) {
          t &= ~fs, t &= ~Gi, e.suspendedLanes |= t, e.pingedLanes &= ~t, r && (e.warmLanes |= t), r = e.expirationTimes;
          for (var s = t; 0 < s; ) {
            var f = 31 - wt(s), v = 1 << f;
            r[f] = -1, s &= ~v;
          }
          n !== 0 && ef(e, n, t);
        }
        function ql() {
          return (He & 6) === 0 ? (Sr(0), false) : true;
        }
        function ps() {
          if (Oe !== null) {
            if (Fe === 0) var e = Oe.return;
            else e = Oe, On = Ni = null, Mu(e), fa = null, hr = 0, e = Oe;
            for (; e !== null; ) Sh(e.alternate, e), e = e.return;
            Oe = null;
          }
        }
        function ba(e, t) {
          var n = e.timeoutHandle;
          n !== -1 && (e.timeoutHandle = -1, Ky(n)), n = e.cancelPendingCommit, n !== null && (e.cancelPendingCommit = null, n()), ps(), Pe = e, Oe = n = Cn(e.current, null), Ge = t, Fe = 0, Ft = null, li = false, ma = Ba(e, t), cs = false, va = tn = fs = Gi = oi = at = 0, Gt = wr = null, ds = false, (t & 8) !== 0 && (t |= t & 32);
          var r = e.entangledLanes;
          if (r !== 0) for (e = e.entanglements, r &= t; 0 < r; ) {
            var s = 31 - wt(r), f = 1 << s;
            t |= e[s], r &= ~f;
          }
          return Bn = t, cl(), n;
        }
        function $h(e, t) {
          Ce = null, D.H = Dl, t === ar || t === bl ? (t = ud(), Fe = 3) : t === rd ? (t = ud(), Fe = 4) : Fe = t === sh ? 8 : t !== null && typeof t == "object" && typeof t.then == "function" ? 6 : 1, Ft = t, Oe === null && (at = 1, Ml(e, Pt(t, e.current)));
        }
        function Xh() {
          var e = D.H;
          return D.H = Dl, e === null ? Dl : e;
        }
        function Zh() {
          var e = D.A;
          return D.A = Dy, e;
        }
        function ys() {
          at = 4, li || (Ge & 4194048) !== Ge && en.current !== null || (ma = true), (oi & 134217727) === 0 && (Gi & 134217727) === 0 || Pe === null || ci(Pe, Ge, tn, false);
        }
        function bs(e, t, n) {
          var r = He;
          He |= 2;
          var s = Xh(), f = Zh();
          (Pe !== e || Ge !== t) && (Fl = null, ba(e, t)), t = false;
          var v = at;
          e: do
            try {
              if (Fe !== 0 && Oe !== null) {
                var b = Oe, A = Ft;
                switch (Fe) {
                  case 8:
                    ps(), v = 6;
                    break e;
                  case 3:
                  case 2:
                  case 9:
                  case 6:
                    en.current === null && (t = true);
                    var j = Fe;
                    if (Fe = 0, Ft = null, _a(e, b, A, j), n && ma) {
                      v = 0;
                      break e;
                    }
                    break;
                  default:
                    j = Fe, Fe = 0, Ft = null, _a(e, b, A, j);
                }
              }
              zy(), v = at;
              break;
            } catch (Q) {
              $h(e, Q);
            }
          while (true);
          return t && e.shellSuspendCounter++, On = Ni = null, He = r, D.H = s, D.A = f, Oe === null && (Pe = null, Ge = 0, cl()), v;
        }
        function zy() {
          for (; Oe !== null; ) Qh(Oe);
        }
        function ky(e, t) {
          var n = He;
          He |= 2;
          var r = Xh(), s = Zh();
          Pe !== e || Ge !== t ? (Fl = null, Hl = nt() + 500, ba(e, t)) : ma = Ba(e, t);
          e: do
            try {
              if (Fe !== 0 && Oe !== null) {
                t = Oe;
                var f = Ft;
                t: switch (Fe) {
                  case 1:
                    Fe = 0, Ft = null, _a(e, t, f, 1);
                    break;
                  case 2:
                  case 9:
                    if (ld(f)) {
                      Fe = 0, Ft = null, Kh(t);
                      break;
                    }
                    t = function() {
                      Fe !== 2 && Fe !== 9 || Pe !== e || (Fe = 7), Sn(e);
                    }, f.then(t, t);
                    break e;
                  case 3:
                    Fe = 7;
                    break e;
                  case 4:
                    Fe = 5;
                    break e;
                  case 7:
                    ld(f) ? (Fe = 0, Ft = null, Kh(t)) : (Fe = 0, Ft = null, _a(e, t, f, 7));
                    break;
                  case 5:
                    var v = null;
                    switch (Oe.tag) {
                      case 26:
                        v = Oe.memoizedState;
                      case 5:
                      case 27:
                        var b = Oe;
                        if (!v || Ng(v)) {
                          Fe = 0, Ft = null;
                          var A = b.sibling;
                          if (A !== null) Oe = A;
                          else {
                            var j = b.return;
                            j !== null ? (Oe = j, Vl(j)) : Oe = null;
                          }
                          break t;
                        }
                    }
                    Fe = 0, Ft = null, _a(e, t, f, 5);
                    break;
                  case 6:
                    Fe = 0, Ft = null, _a(e, t, f, 6);
                    break;
                  case 8:
                    ps(), at = 6;
                    break e;
                  default:
                    throw Error(i(462));
                }
              }
              My();
              break;
            } catch (Q) {
              $h(e, Q);
            }
          while (true);
          return On = Ni = null, D.H = r, D.A = s, He = n, Oe !== null ? 0 : (Pe = null, Ge = 0, cl(), at);
        }
        function My() {
          for (; Oe !== null && !je(); ) Qh(Oe);
        }
        function Qh(e) {
          var t = wh(e.alternate, e, Bn);
          e.memoizedProps = e.pendingProps, t === null ? Vl(e) : Oe = t;
        }
        function Kh(e) {
          var t = e, n = t.alternate;
          switch (t.tag) {
            case 15:
            case 0:
              t = mh(n, t, t.pendingProps, t.type, void 0, Ge);
              break;
            case 11:
              t = mh(n, t, t.pendingProps, t.type.render, t.ref, Ge);
              break;
            case 5:
              Mu(t);
            default:
              Sh(n, t), t = Oe = Pf(t, Bn), t = wh(n, t, Bn);
          }
          e.memoizedProps = e.pendingProps, t === null ? Vl(e) : Oe = t;
        }
        function _a(e, t, n, r) {
          On = Ni = null, Mu(t), fa = null, hr = 0;
          var s = t.return;
          try {
            if (Sy(e, s, t, n, Ge)) {
              at = 1, Ml(e, Pt(n, e.current)), Oe = null;
              return;
            }
          } catch (f) {
            if (s !== null) throw Oe = s, f;
            at = 1, Ml(e, Pt(n, e.current)), Oe = null;
            return;
          }
          t.flags & 32768 ? (Be || r === 1 ? e = true : ma || (Ge & 536870912) !== 0 ? e = false : (li = e = true, (r === 2 || r === 9 || r === 3 || r === 6) && (r = en.current, r !== null && r.tag === 13 && (r.flags |= 16384))), Ph(t, e)) : Vl(t);
        }
        function Vl(e) {
          var t = e;
          do {
            if ((t.flags & 32768) !== 0) {
              Ph(t, li);
              return;
            }
            e = t.return;
            var n = Ty(t.alternate, t, Bn);
            if (n !== null) {
              Oe = n;
              return;
            }
            if (t = t.sibling, t !== null) {
              Oe = t;
              return;
            }
            Oe = t = e;
          } while (t !== null);
          at === 0 && (at = 5);
        }
        function Ph(e, t) {
          do {
            var n = Ay(e.alternate, e);
            if (n !== null) {
              n.flags &= 32767, Oe = n;
              return;
            }
            if (n = e.return, n !== null && (n.flags |= 32768, n.subtreeFlags = 0, n.deletions = null), !t && (e = e.sibling, e !== null)) {
              Oe = e;
              return;
            }
            Oe = e = n;
          } while (e !== null);
          at = 6, Oe = null;
        }
        function Ih(e, t, n, r, s, f, v, b, A) {
          e.cancelPendingCommit = null;
          do
            Yl();
          while (St !== 0);
          if ((He & 6) !== 0) throw Error(i(327));
          if (t !== null) {
            if (t === e.current) throw Error(i(177));
            if (f = t.lanes | t.childLanes, f |= su, dp(e, n, f, v, b, A), e === Pe && (Oe = Pe = null, Ge = 0), pa = t, si = e, ya = n, gs = f, ms = s, Fh = r, (t.subtreeFlags & 10256) !== 0 || (t.flags & 10256) !== 0 ? (e.callbackNode = null, e.callbackPriority = 0, jy(sn, function() {
              return ng(), null;
            })) : (e.callbackNode = null, e.callbackPriority = 0), r = (t.flags & 13878) !== 0, (t.subtreeFlags & 13878) !== 0 || r) {
              r = D.T, D.T = null, s = X.p, X.p = 2, v = He, He |= 4;
              try {
                Ry(e, t, n);
              } finally {
                He = v, X.p = s, D.T = r;
              }
            }
            St = 1, Wh(), Jh(), eg();
          }
        }
        function Wh() {
          if (St === 1) {
            St = 0;
            var e = si, t = pa, n = (t.flags & 13878) !== 0;
            if ((t.subtreeFlags & 13878) !== 0 || n) {
              n = D.T, D.T = null;
              var r = X.p;
              X.p = 2;
              var s = He;
              He |= 4;
              try {
                Mh(t, e);
                var f = Os, v = Hf(e.containerInfo), b = f.focusedElem, A = f.selectionRange;
                if (v !== b && b && b.ownerDocument && Bf(b.ownerDocument.documentElement, b)) {
                  if (A !== null && au(b)) {
                    var j = A.start, Q = A.end;
                    if (Q === void 0 && (Q = j), "selectionStart" in b) b.selectionStart = j, b.selectionEnd = Math.min(Q, b.value.length);
                    else {
                      var W = b.ownerDocument || document, F = W && W.defaultView || window;
                      if (F.getSelection) {
                        var q = F.getSelection(), xe = b.textContent.length, we = Math.min(A.start, xe), Xe = A.end === void 0 ? we : Math.min(A.end, xe);
                        !q.extend && we > Xe && (v = Xe, Xe = we, we = v);
                        var O = jf(b, we), C = jf(b, Xe);
                        if (O && C && (q.rangeCount !== 1 || q.anchorNode !== O.node || q.anchorOffset !== O.offset || q.focusNode !== C.node || q.focusOffset !== C.offset)) {
                          var M = W.createRange();
                          M.setStart(O.node, O.offset), q.removeAllRanges(), we > Xe ? (q.addRange(M), q.extend(C.node, C.offset)) : (M.setEnd(C.node, C.offset), q.addRange(M));
                        }
                      }
                    }
                  }
                  for (W = [], q = b; q = q.parentNode; ) q.nodeType === 1 && W.push({
                    element: q,
                    left: q.scrollLeft,
                    top: q.scrollTop
                  });
                  for (typeof b.focus == "function" && b.focus(), b = 0; b < W.length; b++) {
                    var P = W[b];
                    P.element.scrollLeft = P.left, P.element.scrollTop = P.top;
                  }
                }
                no = !!Ns, Os = Ns = null;
              } finally {
                He = s, X.p = r, D.T = n;
              }
            }
            e.current = t, St = 2;
          }
        }
        function Jh() {
          if (St === 2) {
            St = 0;
            var e = si, t = pa, n = (t.flags & 8772) !== 0;
            if ((t.subtreeFlags & 8772) !== 0 || n) {
              n = D.T, D.T = null;
              var r = X.p;
              X.p = 2;
              var s = He;
              He |= 4;
              try {
                Nh(e, t.alternate, t);
              } finally {
                He = s, X.p = r, D.T = n;
              }
            }
            St = 3;
          }
        }
        function eg() {
          if (St === 4 || St === 3) {
            St = 0, Ae();
            var e = si, t = pa, n = ya, r = Fh;
            (t.subtreeFlags & 10256) !== 0 || (t.flags & 10256) !== 0 ? St = 5 : (St = 0, pa = si = null, tg(e, e.pendingLanes));
            var s = e.pendingLanes;
            if (s === 0 && (ui = null), Uo(n), t = t.stateNode, Ve && typeof Ve.onCommitFiberRoot == "function") try {
              Ve.onCommitFiberRoot(Je, t, void 0, (t.current.flags & 128) === 128);
            } catch {
            }
            if (r !== null) {
              t = D.T, s = X.p, X.p = 2, D.T = null;
              try {
                for (var f = e.onRecoverableError, v = 0; v < r.length; v++) {
                  var b = r[v];
                  f(b.value, {
                    componentStack: b.stack
                  });
                }
              } finally {
                D.T = t, X.p = s;
              }
            }
            (ya & 3) !== 0 && Yl(), Sn(e), s = e.pendingLanes, (n & 4194090) !== 0 && (s & 42) !== 0 ? e === vs ? Er++ : (Er = 0, vs = e) : Er = 0, Sr(0);
          }
        }
        function tg(e, t) {
          (e.pooledCacheLanes &= t) === 0 && (t = e.pooledCache, t != null && (e.pooledCache = null, nr(t)));
        }
        function Yl(e) {
          return Wh(), Jh(), eg(), ng();
        }
        function ng() {
          if (St !== 5) return false;
          var e = si, t = gs;
          gs = 0;
          var n = Uo(ya), r = D.T, s = X.p;
          try {
            X.p = 32 > n ? 32 : n, D.T = null, n = ms, ms = null;
            var f = si, v = ya;
            if (St = 0, pa = si = null, ya = 0, (He & 6) !== 0) throw Error(i(331));
            var b = He;
            if (He |= 4, Bh(f.current), Gh(f, f.current, v, n), He = b, Sr(0, false), Ve && typeof Ve.onPostCommitFiberRoot == "function") try {
              Ve.onPostCommitFiberRoot(Je, f);
            } catch {
            }
            return true;
          } finally {
            X.p = s, D.T = r, tg(e, t);
          }
        }
        function ig(e, t, n) {
          t = Pt(n, t), t = Qu(e.stateNode, t, 2), e = Jn(e, t, 2), e !== null && (Ha(e, 2), Sn(e));
        }
        function Qe(e, t, n) {
          if (e.tag === 3) ig(e, e, n);
          else for (; t !== null; ) {
            if (t.tag === 3) {
              ig(t, e, n);
              break;
            } else if (t.tag === 1) {
              var r = t.stateNode;
              if (typeof t.type.getDerivedStateFromError == "function" || typeof r.componentDidCatch == "function" && (ui === null || !ui.has(r))) {
                e = Pt(n, e), n = oh(2), r = Jn(t, n, 2), r !== null && (uh(n, r, t, e), Ha(r, 2), Sn(r));
                break;
              }
            }
            t = t.return;
          }
        }
        function _s(e, t, n) {
          var r = e.pingCache;
          if (r === null) {
            r = e.pingCache = new Ny();
            var s = /* @__PURE__ */ new Set();
            r.set(t, s);
          } else s = r.get(t), s === void 0 && (s = /* @__PURE__ */ new Set(), r.set(t, s));
          s.has(n) || (cs = true, s.add(n), e = Ly.bind(null, e, t, n), t.then(e, e));
        }
        function Ly(e, t, n) {
          var r = e.pingCache;
          r !== null && r.delete(t), e.pingedLanes |= e.suspendedLanes & n, e.warmLanes &= ~n, Pe === e && (Ge & n) === n && (at === 4 || at === 3 && (Ge & 62914560) === Ge && 300 > nt() - hs ? (He & 2) === 0 && ba(e, 0) : fs |= n, va === Ge && (va = 0)), Sn(e);
        }
        function ag(e, t) {
          t === 0 && (t = Jc()), e = ta(e, t), e !== null && (Ha(e, t), Sn(e));
        }
        function Gy(e) {
          var t = e.memoizedState, n = 0;
          t !== null && (n = t.retryLane), ag(e, n);
        }
        function Uy(e, t) {
          var n = 0;
          switch (e.tag) {
            case 13:
              var r = e.stateNode, s = e.memoizedState;
              s !== null && (n = s.retryLane);
              break;
            case 19:
              r = e.stateNode;
              break;
            case 22:
              r = e.stateNode._retryCache;
              break;
            default:
              throw Error(i(314));
          }
          r !== null && r.delete(t), ag(e, n);
        }
        function jy(e, t) {
          return Ze(e, t);
        }
        var $l = null, wa = null, ws = false, Xl = false, Es = false, Ui = 0;
        function Sn(e) {
          e !== wa && e.next === null && (wa === null ? $l = wa = e : wa = wa.next = e), Xl = true, ws || (ws = true, Hy());
        }
        function Sr(e, t) {
          if (!Es && Xl) {
            Es = true;
            do
              for (var n = false, r = $l; r !== null; ) {
                if (e !== 0) {
                  var s = r.pendingLanes;
                  if (s === 0) var f = 0;
                  else {
                    var v = r.suspendedLanes, b = r.pingedLanes;
                    f = (1 << 31 - wt(42 | e) + 1) - 1, f &= s & ~(v & ~b), f = f & 201326741 ? f & 201326741 | 1 : f ? f | 2 : 0;
                  }
                  f !== 0 && (n = true, ug(r, f));
                } else f = Ge, f = Wr(r, r === Pe ? f : 0, r.cancelPendingCommit !== null || r.timeoutHandle !== -1), (f & 3) === 0 || Ba(r, f) || (n = true, ug(r, f));
                r = r.next;
              }
            while (n);
            Es = false;
          }
        }
        function By() {
          rg();
        }
        function rg() {
          Xl = ws = false;
          var e = 0;
          Ui !== 0 && (Qy() && (e = Ui), Ui = 0);
          for (var t = nt(), n = null, r = $l; r !== null; ) {
            var s = r.next, f = lg(r, t);
            f === 0 ? (r.next = null, n === null ? $l = s : n.next = s, s === null && (wa = n)) : (n = r, (e !== 0 || (f & 3) !== 0) && (Xl = true)), r = s;
          }
          Sr(e);
        }
        function lg(e, t) {
          for (var n = e.suspendedLanes, r = e.pingedLanes, s = e.expirationTimes, f = e.pendingLanes & -62914561; 0 < f; ) {
            var v = 31 - wt(f), b = 1 << v, A = s[v];
            A === -1 ? ((b & n) === 0 || (b & r) !== 0) && (s[v] = fp(b, t)) : A <= t && (e.expiredLanes |= b), f &= ~b;
          }
          if (t = Pe, n = Ge, n = Wr(e, e === t ? n : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), r = e.callbackNode, n === 0 || e === t && (Fe === 2 || Fe === 9) || e.cancelPendingCommit !== null) return r !== null && r !== null && Le(r), e.callbackNode = null, e.callbackPriority = 0;
          if ((n & 3) === 0 || Ba(e, n)) {
            if (t = n & -n, t === e.callbackPriority) return t;
            switch (r !== null && Le(r), Uo(n)) {
              case 2:
              case 8:
                n = un;
                break;
              case 32:
                n = sn;
                break;
              case 268435456:
                n = Xn;
                break;
              default:
                n = sn;
            }
            return r = og.bind(null, e), n = Ze(n, r), e.callbackPriority = t, e.callbackNode = n, t;
          }
          return r !== null && r !== null && Le(r), e.callbackPriority = 2, e.callbackNode = null, 2;
        }
        function og(e, t) {
          if (St !== 0 && St !== 5) return e.callbackNode = null, e.callbackPriority = 0, null;
          var n = e.callbackNode;
          if (Yl() && e.callbackNode !== n) return null;
          var r = Ge;
          return r = Wr(e, e === Pe ? r : 0, e.cancelPendingCommit !== null || e.timeoutHandle !== -1), r === 0 ? null : (Vh(e, r, t), lg(e, nt()), e.callbackNode != null && e.callbackNode === n ? og.bind(null, e) : null);
        }
        function ug(e, t) {
          if (Yl()) return null;
          Vh(e, t, true);
        }
        function Hy() {
          Py(function() {
            (He & 6) !== 0 ? Ze(Ke, By) : rg();
          });
        }
        function Ss() {
          return Ui === 0 && (Ui = Wc()), Ui;
        }
        function sg(e) {
          return e == null || typeof e == "symbol" || typeof e == "boolean" ? null : typeof e == "function" ? e : il("" + e);
        }
        function cg(e, t) {
          var n = t.ownerDocument.createElement("input");
          return n.name = t.name, n.value = t.value, e.id && n.setAttribute("form", e.id), t.parentNode.insertBefore(n, t), e = new FormData(e), n.parentNode.removeChild(n), e;
        }
        function Fy(e, t, n, r, s) {
          if (t === "submit" && n && n.stateNode === s) {
            var f = sg((s[zt] || null).action), v = r.submitter;
            v && (t = (t = v[zt] || null) ? sg(t.formAction) : v.getAttribute("formAction"), t !== null && (f = t, v = null));
            var b = new ol("action", "action", null, r, s);
            e.push({
              event: b,
              listeners: [
                {
                  instance: null,
                  listener: function() {
                    if (r.defaultPrevented) {
                      if (Ui !== 0) {
                        var A = v ? cg(s, v) : new FormData(s);
                        Vu(n, {
                          pending: true,
                          data: A,
                          method: s.method,
                          action: f
                        }, null, A);
                      }
                    } else typeof f == "function" && (b.preventDefault(), A = v ? cg(s, v) : new FormData(s), Vu(n, {
                      pending: true,
                      data: A,
                      method: s.method,
                      action: f
                    }, f, A));
                  },
                  currentTarget: s
                }
              ]
            });
          }
        }
        for (var xs = 0; xs < uu.length; xs++) {
          var Ts = uu[xs], qy = Ts.toLowerCase(), Vy = Ts[0].toUpperCase() + Ts.slice(1);
          cn(qy, "on" + Vy);
        }
        cn(Vf, "onAnimationEnd"), cn(Yf, "onAnimationIteration"), cn($f, "onAnimationStart"), cn("dblclick", "onDoubleClick"), cn("focusin", "onFocus"), cn("focusout", "onBlur"), cn(ly, "onTransitionRun"), cn(oy, "onTransitionStart"), cn(uy, "onTransitionCancel"), cn(Xf, "onTransitionEnd"), $i("onMouseEnter", [
          "mouseout",
          "mouseover"
        ]), $i("onMouseLeave", [
          "mouseout",
          "mouseover"
        ]), $i("onPointerEnter", [
          "pointerout",
          "pointerover"
        ]), $i("onPointerLeave", [
          "pointerout",
          "pointerover"
        ]), wi("onChange", "change click focusin focusout input keydown keyup selectionchange".split(" ")), wi("onSelect", "focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" ")), wi("onBeforeInput", [
          "compositionend",
          "keypress",
          "textInput",
          "paste"
        ]), wi("onCompositionEnd", "compositionend focusout keydown keypress keyup mousedown".split(" ")), wi("onCompositionStart", "compositionstart focusout keydown keypress keyup mousedown".split(" ")), wi("onCompositionUpdate", "compositionupdate focusout keydown keypress keyup mousedown".split(" "));
        var xr = "abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "), Yy = new Set("beforetoggle cancel close invalid load scroll scrollend toggle".split(" ").concat(xr));
        function fg(e, t) {
          t = (t & 4) !== 0;
          for (var n = 0; n < e.length; n++) {
            var r = e[n], s = r.event;
            r = r.listeners;
            e: {
              var f = void 0;
              if (t) for (var v = r.length - 1; 0 <= v; v--) {
                var b = r[v], A = b.instance, j = b.currentTarget;
                if (b = b.listener, A !== f && s.isPropagationStopped()) break e;
                f = b, s.currentTarget = j;
                try {
                  f(s);
                } catch (Q) {
                  kl(Q);
                }
                s.currentTarget = null, f = A;
              }
              else for (v = 0; v < r.length; v++) {
                if (b = r[v], A = b.instance, j = b.currentTarget, b = b.listener, A !== f && s.isPropagationStopped()) break e;
                f = b, s.currentTarget = j;
                try {
                  f(s);
                } catch (Q) {
                  kl(Q);
                }
                s.currentTarget = null, f = A;
              }
            }
          }
        }
        function ze(e, t) {
          var n = t[jo];
          n === void 0 && (n = t[jo] = /* @__PURE__ */ new Set());
          var r = e + "__bubble";
          n.has(r) || (dg(t, e, 2, false), n.add(r));
        }
        function As(e, t, n) {
          var r = 0;
          t && (r |= 4), dg(n, e, r, t);
        }
        var Zl = "_reactListening" + Math.random().toString(36).slice(2);
        function Rs(e) {
          if (!e[Zl]) {
            e[Zl] = true, rf.forEach(function(n) {
              n !== "selectionchange" && (Yy.has(n) || As(n, false, e), As(n, true, e));
            });
            var t = e.nodeType === 9 ? e : e.ownerDocument;
            t === null || t[Zl] || (t[Zl] = true, As("selectionchange", false, t));
          }
        }
        function dg(e, t, n, r) {
          switch (Gg(t)) {
            case 2:
              var s = pb;
              break;
            case 8:
              s = yb;
              break;
            default:
              s = Fs;
          }
          n = s.bind(null, t, n, e), s = void 0, !Ko || t !== "touchstart" && t !== "touchmove" && t !== "wheel" || (s = true), r ? s !== void 0 ? e.addEventListener(t, n, {
            capture: true,
            passive: s
          }) : e.addEventListener(t, n, true) : s !== void 0 ? e.addEventListener(t, n, {
            passive: s
          }) : e.addEventListener(t, n, false);
        }
        function Cs(e, t, n, r, s) {
          var f = r;
          if ((t & 1) === 0 && (t & 2) === 0 && r !== null) e: for (; ; ) {
            if (r === null) return;
            var v = r.tag;
            if (v === 3 || v === 4) {
              var b = r.stateNode.containerInfo;
              if (b === s) break;
              if (v === 4) for (v = r.return; v !== null; ) {
                var A = v.tag;
                if ((A === 3 || A === 4) && v.stateNode.containerInfo === s) return;
                v = v.return;
              }
              for (; b !== null; ) {
                if (v = qi(b), v === null) return;
                if (A = v.tag, A === 5 || A === 6 || A === 26 || A === 27) {
                  r = f = v;
                  continue e;
                }
                b = b.parentNode;
              }
            }
            r = r.return;
          }
          bf(function() {
            var j = f, Q = Zo(n), W = [];
            e: {
              var F = Zf.get(e);
              if (F !== void 0) {
                var q = ol, xe = e;
                switch (e) {
                  case "keypress":
                    if (rl(n) === 0) break e;
                  case "keydown":
                  case "keyup":
                    q = jp;
                    break;
                  case "focusin":
                    xe = "focus", q = Jo;
                    break;
                  case "focusout":
                    xe = "blur", q = Jo;
                    break;
                  case "beforeblur":
                  case "afterblur":
                    q = Jo;
                    break;
                  case "click":
                    if (n.button === 2) break e;
                  case "auxclick":
                  case "dblclick":
                  case "mousedown":
                  case "mousemove":
                  case "mouseup":
                  case "mouseout":
                  case "mouseover":
                  case "contextmenu":
                    q = Ef;
                    break;
                  case "drag":
                  case "dragend":
                  case "dragenter":
                  case "dragexit":
                  case "dragleave":
                  case "dragover":
                  case "dragstart":
                  case "drop":
                    q = Ap;
                    break;
                  case "touchcancel":
                  case "touchend":
                  case "touchmove":
                  case "touchstart":
                    q = Fp;
                    break;
                  case Vf:
                  case Yf:
                  case $f:
                    q = Dp;
                    break;
                  case Xf:
                    q = Vp;
                    break;
                  case "scroll":
                  case "scrollend":
                    q = xp;
                    break;
                  case "wheel":
                    q = $p;
                    break;
                  case "copy":
                  case "cut":
                  case "paste":
                    q = Op;
                    break;
                  case "gotpointercapture":
                  case "lostpointercapture":
                  case "pointercancel":
                  case "pointerdown":
                  case "pointermove":
                  case "pointerout":
                  case "pointerover":
                  case "pointerup":
                    q = xf;
                    break;
                  case "toggle":
                  case "beforetoggle":
                    q = Zp;
                }
                var we = (t & 4) !== 0, Xe = !we && (e === "scroll" || e === "scrollend"), O = we ? F !== null ? F + "Capture" : null : F;
                we = [];
                for (var C = j, M; C !== null; ) {
                  var P = C;
                  if (M = P.stateNode, P = P.tag, P !== 5 && P !== 26 && P !== 27 || M === null || O === null || (P = Va(C, O), P != null && we.push(Tr(C, P, M))), Xe) break;
                  C = C.return;
                }
                0 < we.length && (F = new q(F, xe, null, n, Q), W.push({
                  event: F,
                  listeners: we
                }));
              }
            }
            if ((t & 7) === 0) {
              e: {
                if (F = e === "mouseover" || e === "pointerover", q = e === "mouseout" || e === "pointerout", F && n !== Xo && (xe = n.relatedTarget || n.fromElement) && (qi(xe) || xe[Fi])) break e;
                if ((q || F) && (F = Q.window === Q ? Q : (F = Q.ownerDocument) ? F.defaultView || F.parentWindow : window, q ? (xe = n.relatedTarget || n.toElement, q = j, xe = xe ? qi(xe) : null, xe !== null && (Xe = c(xe), we = xe.tag, xe !== Xe || we !== 5 && we !== 27 && we !== 6) && (xe = null)) : (q = null, xe = j), q !== xe)) {
                  if (we = Ef, P = "onMouseLeave", O = "onMouseEnter", C = "mouse", (e === "pointerout" || e === "pointerover") && (we = xf, P = "onPointerLeave", O = "onPointerEnter", C = "pointer"), Xe = q == null ? F : qa(q), M = xe == null ? F : qa(xe), F = new we(P, C + "leave", q, n, Q), F.target = Xe, F.relatedTarget = M, P = null, qi(Q) === j && (we = new we(O, C + "enter", xe, n, Q), we.target = M, we.relatedTarget = Xe, P = we), Xe = P, q && xe) t: {
                    for (we = q, O = xe, C = 0, M = we; M; M = Ea(M)) C++;
                    for (M = 0, P = O; P; P = Ea(P)) M++;
                    for (; 0 < C - M; ) we = Ea(we), C--;
                    for (; 0 < M - C; ) O = Ea(O), M--;
                    for (; C--; ) {
                      if (we === O || O !== null && we === O.alternate) break t;
                      we = Ea(we), O = Ea(O);
                    }
                    we = null;
                  }
                  else we = null;
                  q !== null && hg(W, F, q, we, false), xe !== null && Xe !== null && hg(W, Xe, xe, we, true);
                }
              }
              e: {
                if (F = j ? qa(j) : window, q = F.nodeName && F.nodeName.toLowerCase(), q === "select" || q === "input" && F.type === "file") var he = zf;
                else if (Nf(F)) if (kf) he = iy;
                else {
                  he = ty;
                  var Ne = ey;
                }
                else q = F.nodeName, !q || q.toLowerCase() !== "input" || F.type !== "checkbox" && F.type !== "radio" ? j && $o(j.elementType) && (he = zf) : he = ny;
                if (he && (he = he(e, j))) {
                  Of(W, he, n, Q);
                  break e;
                }
                Ne && Ne(e, F, j), e === "focusout" && j && F.type === "number" && j.memoizedProps.value != null && Yo(F, "number", F.value);
              }
              switch (Ne = j ? qa(j) : window, e) {
                case "focusin":
                  (Nf(Ne) || Ne.contentEditable === "true") && (Wi = Ne, ru = j, Ia = null);
                  break;
                case "focusout":
                  Ia = ru = Wi = null;
                  break;
                case "mousedown":
                  lu = true;
                  break;
                case "contextmenu":
                case "mouseup":
                case "dragend":
                  lu = false, Ff(W, n, Q);
                  break;
                case "selectionchange":
                  if (ry) break;
                case "keydown":
                case "keyup":
                  Ff(W, n, Q);
              }
              var ye;
              if (tu) e: {
                switch (e) {
                  case "compositionstart":
                    var Se = "onCompositionStart";
                    break e;
                  case "compositionend":
                    Se = "onCompositionEnd";
                    break e;
                  case "compositionupdate":
                    Se = "onCompositionUpdate";
                    break e;
                }
                Se = void 0;
              }
              else Ii ? Cf(e, n) && (Se = "onCompositionEnd") : e === "keydown" && n.keyCode === 229 && (Se = "onCompositionStart");
              Se && (Tf && n.locale !== "ko" && (Ii || Se !== "onCompositionStart" ? Se === "onCompositionEnd" && Ii && (ye = _f()) : (Kn = Q, Po = "value" in Kn ? Kn.value : Kn.textContent, Ii = true)), Ne = Ql(j, Se), 0 < Ne.length && (Se = new Sf(Se, e, null, n, Q), W.push({
                event: Se,
                listeners: Ne
              }), ye ? Se.data = ye : (ye = Df(n), ye !== null && (Se.data = ye)))), (ye = Kp ? Pp(e, n) : Ip(e, n)) && (Se = Ql(j, "onBeforeInput"), 0 < Se.length && (Ne = new Sf("onBeforeInput", "beforeinput", null, n, Q), W.push({
                event: Ne,
                listeners: Se
              }), Ne.data = ye)), Fy(W, e, j, n, Q);
            }
            fg(W, t);
          });
        }
        function Tr(e, t, n) {
          return {
            instance: e,
            listener: t,
            currentTarget: n
          };
        }
        function Ql(e, t) {
          for (var n = t + "Capture", r = []; e !== null; ) {
            var s = e, f = s.stateNode;
            if (s = s.tag, s !== 5 && s !== 26 && s !== 27 || f === null || (s = Va(e, n), s != null && r.unshift(Tr(e, s, f)), s = Va(e, t), s != null && r.push(Tr(e, s, f))), e.tag === 3) return r;
            e = e.return;
          }
          return [];
        }
        function Ea(e) {
          if (e === null) return null;
          do
            e = e.return;
          while (e && e.tag !== 5 && e.tag !== 27);
          return e || null;
        }
        function hg(e, t, n, r, s) {
          for (var f = t._reactName, v = []; n !== null && n !== r; ) {
            var b = n, A = b.alternate, j = b.stateNode;
            if (b = b.tag, A !== null && A === r) break;
            b !== 5 && b !== 26 && b !== 27 || j === null || (A = j, s ? (j = Va(n, f), j != null && v.unshift(Tr(n, j, A))) : s || (j = Va(n, f), j != null && v.push(Tr(n, j, A)))), n = n.return;
          }
          v.length !== 0 && e.push({
            event: t,
            listeners: v
          });
        }
        var $y = /\r\n?/g, Xy = /\u0000|\uFFFD/g;
        function gg(e) {
          return (typeof e == "string" ? e : "" + e).replace($y, `
`).replace(Xy, "");
        }
        function mg(e, t) {
          return t = gg(t), gg(e) === t;
        }
        function Kl() {
        }
        function $e(e, t, n, r, s, f) {
          switch (n) {
            case "children":
              typeof r == "string" ? t === "body" || t === "textarea" && r === "" || Qi(e, r) : (typeof r == "number" || typeof r == "bigint") && t !== "body" && Qi(e, "" + r);
              break;
            case "className":
              el(e, "class", r);
              break;
            case "tabIndex":
              el(e, "tabindex", r);
              break;
            case "dir":
            case "role":
            case "viewBox":
            case "width":
            case "height":
              el(e, n, r);
              break;
            case "style":
              pf(e, r, f);
              break;
            case "data":
              if (t !== "object") {
                el(e, "data", r);
                break;
              }
            case "src":
            case "href":
              if (r === "" && (t !== "a" || n !== "href")) {
                e.removeAttribute(n);
                break;
              }
              if (r == null || typeof r == "function" || typeof r == "symbol" || typeof r == "boolean") {
                e.removeAttribute(n);
                break;
              }
              r = il("" + r), e.setAttribute(n, r);
              break;
            case "action":
            case "formAction":
              if (typeof r == "function") {
                e.setAttribute(n, "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')");
                break;
              } else typeof f == "function" && (n === "formAction" ? (t !== "input" && $e(e, t, "name", s.name, s, null), $e(e, t, "formEncType", s.formEncType, s, null), $e(e, t, "formMethod", s.formMethod, s, null), $e(e, t, "formTarget", s.formTarget, s, null)) : ($e(e, t, "encType", s.encType, s, null), $e(e, t, "method", s.method, s, null), $e(e, t, "target", s.target, s, null)));
              if (r == null || typeof r == "symbol" || typeof r == "boolean") {
                e.removeAttribute(n);
                break;
              }
              r = il("" + r), e.setAttribute(n, r);
              break;
            case "onClick":
              r != null && (e.onclick = Kl);
              break;
            case "onScroll":
              r != null && ze("scroll", e);
              break;
            case "onScrollEnd":
              r != null && ze("scrollend", e);
              break;
            case "dangerouslySetInnerHTML":
              if (r != null) {
                if (typeof r != "object" || !("__html" in r)) throw Error(i(61));
                if (n = r.__html, n != null) {
                  if (s.children != null) throw Error(i(60));
                  e.innerHTML = n;
                }
              }
              break;
            case "multiple":
              e.multiple = r && typeof r != "function" && typeof r != "symbol";
              break;
            case "muted":
              e.muted = r && typeof r != "function" && typeof r != "symbol";
              break;
            case "suppressContentEditableWarning":
            case "suppressHydrationWarning":
            case "defaultValue":
            case "defaultChecked":
            case "innerHTML":
            case "ref":
              break;
            case "autoFocus":
              break;
            case "xlinkHref":
              if (r == null || typeof r == "function" || typeof r == "boolean" || typeof r == "symbol") {
                e.removeAttribute("xlink:href");
                break;
              }
              n = il("" + r), e.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", n);
              break;
            case "contentEditable":
            case "spellCheck":
            case "draggable":
            case "value":
            case "autoReverse":
            case "externalResourcesRequired":
            case "focusable":
            case "preserveAlpha":
              r != null && typeof r != "function" && typeof r != "symbol" ? e.setAttribute(n, "" + r) : e.removeAttribute(n);
              break;
            case "inert":
            case "allowFullScreen":
            case "async":
            case "autoPlay":
            case "controls":
            case "default":
            case "defer":
            case "disabled":
            case "disablePictureInPicture":
            case "disableRemotePlayback":
            case "formNoValidate":
            case "hidden":
            case "loop":
            case "noModule":
            case "noValidate":
            case "open":
            case "playsInline":
            case "readOnly":
            case "required":
            case "reversed":
            case "scoped":
            case "seamless":
            case "itemScope":
              r && typeof r != "function" && typeof r != "symbol" ? e.setAttribute(n, "") : e.removeAttribute(n);
              break;
            case "capture":
            case "download":
              r === true ? e.setAttribute(n, "") : r !== false && r != null && typeof r != "function" && typeof r != "symbol" ? e.setAttribute(n, r) : e.removeAttribute(n);
              break;
            case "cols":
            case "rows":
            case "size":
            case "span":
              r != null && typeof r != "function" && typeof r != "symbol" && !isNaN(r) && 1 <= r ? e.setAttribute(n, r) : e.removeAttribute(n);
              break;
            case "rowSpan":
            case "start":
              r == null || typeof r == "function" || typeof r == "symbol" || isNaN(r) ? e.removeAttribute(n) : e.setAttribute(n, r);
              break;
            case "popover":
              ze("beforetoggle", e), ze("toggle", e), Jr(e, "popover", r);
              break;
            case "xlinkActuate":
              An(e, "http://www.w3.org/1999/xlink", "xlink:actuate", r);
              break;
            case "xlinkArcrole":
              An(e, "http://www.w3.org/1999/xlink", "xlink:arcrole", r);
              break;
            case "xlinkRole":
              An(e, "http://www.w3.org/1999/xlink", "xlink:role", r);
              break;
            case "xlinkShow":
              An(e, "http://www.w3.org/1999/xlink", "xlink:show", r);
              break;
            case "xlinkTitle":
              An(e, "http://www.w3.org/1999/xlink", "xlink:title", r);
              break;
            case "xlinkType":
              An(e, "http://www.w3.org/1999/xlink", "xlink:type", r);
              break;
            case "xmlBase":
              An(e, "http://www.w3.org/XML/1998/namespace", "xml:base", r);
              break;
            case "xmlLang":
              An(e, "http://www.w3.org/XML/1998/namespace", "xml:lang", r);
              break;
            case "xmlSpace":
              An(e, "http://www.w3.org/XML/1998/namespace", "xml:space", r);
              break;
            case "is":
              Jr(e, "is", r);
              break;
            case "innerText":
            case "textContent":
              break;
            default:
              (!(2 < n.length) || n[0] !== "o" && n[0] !== "O" || n[1] !== "n" && n[1] !== "N") && (n = Ep.get(n) || n, Jr(e, n, r));
          }
        }
        function Ds(e, t, n, r, s, f) {
          switch (n) {
            case "style":
              pf(e, r, f);
              break;
            case "dangerouslySetInnerHTML":
              if (r != null) {
                if (typeof r != "object" || !("__html" in r)) throw Error(i(61));
                if (n = r.__html, n != null) {
                  if (s.children != null) throw Error(i(60));
                  e.innerHTML = n;
                }
              }
              break;
            case "children":
              typeof r == "string" ? Qi(e, r) : (typeof r == "number" || typeof r == "bigint") && Qi(e, "" + r);
              break;
            case "onScroll":
              r != null && ze("scroll", e);
              break;
            case "onScrollEnd":
              r != null && ze("scrollend", e);
              break;
            case "onClick":
              r != null && (e.onclick = Kl);
              break;
            case "suppressContentEditableWarning":
            case "suppressHydrationWarning":
            case "innerHTML":
            case "ref":
              break;
            case "innerText":
            case "textContent":
              break;
            default:
              if (!lf.hasOwnProperty(n)) e: {
                if (n[0] === "o" && n[1] === "n" && (s = n.endsWith("Capture"), t = n.slice(2, s ? n.length - 7 : void 0), f = e[zt] || null, f = f != null ? f[n] : null, typeof f == "function" && e.removeEventListener(t, f, s), typeof r == "function")) {
                  typeof f != "function" && f !== null && (n in e ? e[n] = null : e.hasAttribute(n) && e.removeAttribute(n)), e.addEventListener(t, r, s);
                  break e;
                }
                n in e ? e[n] = r : r === true ? e.setAttribute(n, "") : Jr(e, n, r);
              }
          }
        }
        function xt(e, t, n) {
          switch (t) {
            case "div":
            case "span":
            case "svg":
            case "path":
            case "a":
            case "g":
            case "p":
            case "li":
              break;
            case "img":
              ze("error", e), ze("load", e);
              var r = false, s = false, f;
              for (f in n) if (n.hasOwnProperty(f)) {
                var v = n[f];
                if (v != null) switch (f) {
                  case "src":
                    r = true;
                    break;
                  case "srcSet":
                    s = true;
                    break;
                  case "children":
                  case "dangerouslySetInnerHTML":
                    throw Error(i(137, t));
                  default:
                    $e(e, t, f, v, n, null);
                }
              }
              s && $e(e, t, "srcSet", n.srcSet, n, null), r && $e(e, t, "src", n.src, n, null);
              return;
            case "input":
              ze("invalid", e);
              var b = f = v = s = null, A = null, j = null;
              for (r in n) if (n.hasOwnProperty(r)) {
                var Q = n[r];
                if (Q != null) switch (r) {
                  case "name":
                    s = Q;
                    break;
                  case "type":
                    v = Q;
                    break;
                  case "checked":
                    A = Q;
                    break;
                  case "defaultChecked":
                    j = Q;
                    break;
                  case "value":
                    f = Q;
                    break;
                  case "defaultValue":
                    b = Q;
                    break;
                  case "children":
                  case "dangerouslySetInnerHTML":
                    if (Q != null) throw Error(i(137, t));
                    break;
                  default:
                    $e(e, t, r, Q, n, null);
                }
              }
              hf(e, f, b, A, j, v, s, false), tl(e);
              return;
            case "select":
              ze("invalid", e), r = v = f = null;
              for (s in n) if (n.hasOwnProperty(s) && (b = n[s], b != null)) switch (s) {
                case "value":
                  f = b;
                  break;
                case "defaultValue":
                  v = b;
                  break;
                case "multiple":
                  r = b;
                default:
                  $e(e, t, s, b, n, null);
              }
              t = f, n = v, e.multiple = !!r, t != null ? Zi(e, !!r, t, false) : n != null && Zi(e, !!r, n, true);
              return;
            case "textarea":
              ze("invalid", e), f = s = r = null;
              for (v in n) if (n.hasOwnProperty(v) && (b = n[v], b != null)) switch (v) {
                case "value":
                  r = b;
                  break;
                case "defaultValue":
                  s = b;
                  break;
                case "children":
                  f = b;
                  break;
                case "dangerouslySetInnerHTML":
                  if (b != null) throw Error(i(91));
                  break;
                default:
                  $e(e, t, v, b, n, null);
              }
              mf(e, r, s, f), tl(e);
              return;
            case "option":
              for (A in n) if (n.hasOwnProperty(A) && (r = n[A], r != null)) switch (A) {
                case "selected":
                  e.selected = r && typeof r != "function" && typeof r != "symbol";
                  break;
                default:
                  $e(e, t, A, r, n, null);
              }
              return;
            case "dialog":
              ze("beforetoggle", e), ze("toggle", e), ze("cancel", e), ze("close", e);
              break;
            case "iframe":
            case "object":
              ze("load", e);
              break;
            case "video":
            case "audio":
              for (r = 0; r < xr.length; r++) ze(xr[r], e);
              break;
            case "image":
              ze("error", e), ze("load", e);
              break;
            case "details":
              ze("toggle", e);
              break;
            case "embed":
            case "source":
            case "link":
              ze("error", e), ze("load", e);
            case "area":
            case "base":
            case "br":
            case "col":
            case "hr":
            case "keygen":
            case "meta":
            case "param":
            case "track":
            case "wbr":
            case "menuitem":
              for (j in n) if (n.hasOwnProperty(j) && (r = n[j], r != null)) switch (j) {
                case "children":
                case "dangerouslySetInnerHTML":
                  throw Error(i(137, t));
                default:
                  $e(e, t, j, r, n, null);
              }
              return;
            default:
              if ($o(t)) {
                for (Q in n) n.hasOwnProperty(Q) && (r = n[Q], r !== void 0 && Ds(e, t, Q, r, n, void 0));
                return;
              }
          }
          for (b in n) n.hasOwnProperty(b) && (r = n[b], r != null && $e(e, t, b, r, n, null));
        }
        function Zy(e, t, n, r) {
          switch (t) {
            case "div":
            case "span":
            case "svg":
            case "path":
            case "a":
            case "g":
            case "p":
            case "li":
              break;
            case "input":
              var s = null, f = null, v = null, b = null, A = null, j = null, Q = null;
              for (q in n) {
                var W = n[q];
                if (n.hasOwnProperty(q) && W != null) switch (q) {
                  case "checked":
                    break;
                  case "value":
                    break;
                  case "defaultValue":
                    A = W;
                  default:
                    r.hasOwnProperty(q) || $e(e, t, q, null, r, W);
                }
              }
              for (var F in r) {
                var q = r[F];
                if (W = n[F], r.hasOwnProperty(F) && (q != null || W != null)) switch (F) {
                  case "type":
                    f = q;
                    break;
                  case "name":
                    s = q;
                    break;
                  case "checked":
                    j = q;
                    break;
                  case "defaultChecked":
                    Q = q;
                    break;
                  case "value":
                    v = q;
                    break;
                  case "defaultValue":
                    b = q;
                    break;
                  case "children":
                  case "dangerouslySetInnerHTML":
                    if (q != null) throw Error(i(137, t));
                    break;
                  default:
                    q !== W && $e(e, t, F, q, r, W);
                }
              }
              Vo(e, v, b, A, j, Q, f, s);
              return;
            case "select":
              q = v = b = F = null;
              for (f in n) if (A = n[f], n.hasOwnProperty(f) && A != null) switch (f) {
                case "value":
                  break;
                case "multiple":
                  q = A;
                default:
                  r.hasOwnProperty(f) || $e(e, t, f, null, r, A);
              }
              for (s in r) if (f = r[s], A = n[s], r.hasOwnProperty(s) && (f != null || A != null)) switch (s) {
                case "value":
                  F = f;
                  break;
                case "defaultValue":
                  b = f;
                  break;
                case "multiple":
                  v = f;
                default:
                  f !== A && $e(e, t, s, f, r, A);
              }
              t = b, n = v, r = q, F != null ? Zi(e, !!n, F, false) : !!r != !!n && (t != null ? Zi(e, !!n, t, true) : Zi(e, !!n, n ? [] : "", false));
              return;
            case "textarea":
              q = F = null;
              for (b in n) if (s = n[b], n.hasOwnProperty(b) && s != null && !r.hasOwnProperty(b)) switch (b) {
                case "value":
                  break;
                case "children":
                  break;
                default:
                  $e(e, t, b, null, r, s);
              }
              for (v in r) if (s = r[v], f = n[v], r.hasOwnProperty(v) && (s != null || f != null)) switch (v) {
                case "value":
                  F = s;
                  break;
                case "defaultValue":
                  q = s;
                  break;
                case "children":
                  break;
                case "dangerouslySetInnerHTML":
                  if (s != null) throw Error(i(91));
                  break;
                default:
                  s !== f && $e(e, t, v, s, r, f);
              }
              gf(e, F, q);
              return;
            case "option":
              for (var xe in n) if (F = n[xe], n.hasOwnProperty(xe) && F != null && !r.hasOwnProperty(xe)) switch (xe) {
                case "selected":
                  e.selected = false;
                  break;
                default:
                  $e(e, t, xe, null, r, F);
              }
              for (A in r) if (F = r[A], q = n[A], r.hasOwnProperty(A) && F !== q && (F != null || q != null)) switch (A) {
                case "selected":
                  e.selected = F && typeof F != "function" && typeof F != "symbol";
                  break;
                default:
                  $e(e, t, A, F, r, q);
              }
              return;
            case "img":
            case "link":
            case "area":
            case "base":
            case "br":
            case "col":
            case "embed":
            case "hr":
            case "keygen":
            case "meta":
            case "param":
            case "source":
            case "track":
            case "wbr":
            case "menuitem":
              for (var we in n) F = n[we], n.hasOwnProperty(we) && F != null && !r.hasOwnProperty(we) && $e(e, t, we, null, r, F);
              for (j in r) if (F = r[j], q = n[j], r.hasOwnProperty(j) && F !== q && (F != null || q != null)) switch (j) {
                case "children":
                case "dangerouslySetInnerHTML":
                  if (F != null) throw Error(i(137, t));
                  break;
                default:
                  $e(e, t, j, F, r, q);
              }
              return;
            default:
              if ($o(t)) {
                for (var Xe in n) F = n[Xe], n.hasOwnProperty(Xe) && F !== void 0 && !r.hasOwnProperty(Xe) && Ds(e, t, Xe, void 0, r, F);
                for (Q in r) F = r[Q], q = n[Q], !r.hasOwnProperty(Q) || F === q || F === void 0 && q === void 0 || Ds(e, t, Q, F, r, q);
                return;
              }
          }
          for (var O in n) F = n[O], n.hasOwnProperty(O) && F != null && !r.hasOwnProperty(O) && $e(e, t, O, null, r, F);
          for (W in r) F = r[W], q = n[W], !r.hasOwnProperty(W) || F === q || F == null && q == null || $e(e, t, W, F, r, q);
        }
        var Ns = null, Os = null;
        function Pl(e) {
          return e.nodeType === 9 ? e : e.ownerDocument;
        }
        function vg(e) {
          switch (e) {
            case "http://www.w3.org/2000/svg":
              return 1;
            case "http://www.w3.org/1998/Math/MathML":
              return 2;
            default:
              return 0;
          }
        }
        function pg(e, t) {
          if (e === 0) switch (t) {
            case "svg":
              return 1;
            case "math":
              return 2;
            default:
              return 0;
          }
          return e === 1 && t === "foreignObject" ? 0 : e;
        }
        function zs(e, t) {
          return e === "textarea" || e === "noscript" || typeof t.children == "string" || typeof t.children == "number" || typeof t.children == "bigint" || typeof t.dangerouslySetInnerHTML == "object" && t.dangerouslySetInnerHTML !== null && t.dangerouslySetInnerHTML.__html != null;
        }
        var ks = null;
        function Qy() {
          var e = window.event;
          return e && e.type === "popstate" ? e === ks ? false : (ks = e, true) : (ks = null, false);
        }
        var yg = typeof setTimeout == "function" ? setTimeout : void 0, Ky = typeof clearTimeout == "function" ? clearTimeout : void 0, bg = typeof Promise == "function" ? Promise : void 0, Py = typeof queueMicrotask == "function" ? queueMicrotask : typeof bg < "u" ? function(e) {
          return bg.resolve(null).then(e).catch(Iy);
        } : yg;
        function Iy(e) {
          setTimeout(function() {
            throw e;
          });
        }
        function fi(e) {
          return e === "head";
        }
        function _g(e, t) {
          var n = t, r = 0, s = 0;
          do {
            var f = n.nextSibling;
            if (e.removeChild(n), f && f.nodeType === 8) if (n = f.data, n === "/$") {
              if (0 < r && 8 > r) {
                n = r;
                var v = e.ownerDocument;
                if (n & 1 && Ar(v.documentElement), n & 2 && Ar(v.body), n & 4) for (n = v.head, Ar(n), v = n.firstChild; v; ) {
                  var b = v.nextSibling, A = v.nodeName;
                  v[Fa] || A === "SCRIPT" || A === "STYLE" || A === "LINK" && v.rel.toLowerCase() === "stylesheet" || n.removeChild(v), v = b;
                }
              }
              if (s === 0) {
                e.removeChild(f), Mr(t);
                return;
              }
              s--;
            } else n === "$" || n === "$?" || n === "$!" ? s++ : r = n.charCodeAt(0) - 48;
            else r = 0;
            n = f;
          } while (n);
          Mr(t);
        }
        function Ms(e) {
          var t = e.firstChild;
          for (t && t.nodeType === 10 && (t = t.nextSibling); t; ) {
            var n = t;
            switch (t = t.nextSibling, n.nodeName) {
              case "HTML":
              case "HEAD":
              case "BODY":
                Ms(n), Bo(n);
                continue;
              case "SCRIPT":
              case "STYLE":
                continue;
              case "LINK":
                if (n.rel.toLowerCase() === "stylesheet") continue;
            }
            e.removeChild(n);
          }
        }
        function Wy(e, t, n, r) {
          for (; e.nodeType === 1; ) {
            var s = n;
            if (e.nodeName.toLowerCase() !== t.toLowerCase()) {
              if (!r && (e.nodeName !== "INPUT" || e.type !== "hidden")) break;
            } else if (r) {
              if (!e[Fa]) switch (t) {
                case "meta":
                  if (!e.hasAttribute("itemprop")) break;
                  return e;
                case "link":
                  if (f = e.getAttribute("rel"), f === "stylesheet" && e.hasAttribute("data-precedence")) break;
                  if (f !== s.rel || e.getAttribute("href") !== (s.href == null || s.href === "" ? null : s.href) || e.getAttribute("crossorigin") !== (s.crossOrigin == null ? null : s.crossOrigin) || e.getAttribute("title") !== (s.title == null ? null : s.title)) break;
                  return e;
                case "style":
                  if (e.hasAttribute("data-precedence")) break;
                  return e;
                case "script":
                  if (f = e.getAttribute("src"), (f !== (s.src == null ? null : s.src) || e.getAttribute("type") !== (s.type == null ? null : s.type) || e.getAttribute("crossorigin") !== (s.crossOrigin == null ? null : s.crossOrigin)) && f && e.hasAttribute("async") && !e.hasAttribute("itemprop")) break;
                  return e;
                default:
                  return e;
              }
            } else if (t === "input" && e.type === "hidden") {
              var f = s.name == null ? null : "" + s.name;
              if (s.type === "hidden" && e.getAttribute("name") === f) return e;
            } else return e;
            if (e = dn(e.nextSibling), e === null) break;
          }
          return null;
        }
        function Jy(e, t, n) {
          if (t === "") return null;
          for (; e.nodeType !== 3; ) if ((e.nodeType !== 1 || e.nodeName !== "INPUT" || e.type !== "hidden") && !n || (e = dn(e.nextSibling), e === null)) return null;
          return e;
        }
        function Ls(e) {
          return e.data === "$!" || e.data === "$?" && e.ownerDocument.readyState === "complete";
        }
        function eb(e, t) {
          var n = e.ownerDocument;
          if (e.data !== "$?" || n.readyState === "complete") t();
          else {
            var r = function() {
              t(), n.removeEventListener("DOMContentLoaded", r);
            };
            n.addEventListener("DOMContentLoaded", r), e._reactRetry = r;
          }
        }
        function dn(e) {
          for (; e != null; e = e.nextSibling) {
            var t = e.nodeType;
            if (t === 1 || t === 3) break;
            if (t === 8) {
              if (t = e.data, t === "$" || t === "$!" || t === "$?" || t === "F!" || t === "F") break;
              if (t === "/$") return null;
            }
          }
          return e;
        }
        var Gs = null;
        function wg(e) {
          e = e.previousSibling;
          for (var t = 0; e; ) {
            if (e.nodeType === 8) {
              var n = e.data;
              if (n === "$" || n === "$!" || n === "$?") {
                if (t === 0) return e;
                t--;
              } else n === "/$" && t++;
            }
            e = e.previousSibling;
          }
          return null;
        }
        function Eg(e, t, n) {
          switch (t = Pl(n), e) {
            case "html":
              if (e = t.documentElement, !e) throw Error(i(452));
              return e;
            case "head":
              if (e = t.head, !e) throw Error(i(453));
              return e;
            case "body":
              if (e = t.body, !e) throw Error(i(454));
              return e;
            default:
              throw Error(i(451));
          }
        }
        function Ar(e) {
          for (var t = e.attributes; t.length; ) e.removeAttributeNode(t[0]);
          Bo(e);
        }
        var nn = /* @__PURE__ */ new Map(), Sg = /* @__PURE__ */ new Set();
        function Il(e) {
          return typeof e.getRootNode == "function" ? e.getRootNode() : e.nodeType === 9 ? e : e.ownerDocument;
        }
        var Hn = X.d;
        X.d = {
          f: tb,
          r: nb,
          D: ib,
          C: ab,
          L: rb,
          m: lb,
          X: ub,
          S: ob,
          M: sb
        };
        function tb() {
          var e = Hn.f(), t = ql();
          return e || t;
        }
        function nb(e) {
          var t = Vi(e);
          t !== null && t.tag === 5 && t.type === "form" ? Vd(t) : Hn.r(e);
        }
        var Sa = typeof document > "u" ? null : document;
        function xg(e, t, n) {
          var r = Sa;
          if (r && typeof t == "string" && t) {
            var s = Kt(t);
            s = 'link[rel="' + e + '"][href="' + s + '"]', typeof n == "string" && (s += '[crossorigin="' + n + '"]'), Sg.has(s) || (Sg.add(s), e = {
              rel: e,
              crossOrigin: n,
              href: t
            }, r.querySelector(s) === null && (t = r.createElement("link"), xt(t, "link", e), pt(t), r.head.appendChild(t)));
          }
        }
        function ib(e) {
          Hn.D(e), xg("dns-prefetch", e, null);
        }
        function ab(e, t) {
          Hn.C(e, t), xg("preconnect", e, t);
        }
        function rb(e, t, n) {
          Hn.L(e, t, n);
          var r = Sa;
          if (r && e && t) {
            var s = 'link[rel="preload"][as="' + Kt(t) + '"]';
            t === "image" && n && n.imageSrcSet ? (s += '[imagesrcset="' + Kt(n.imageSrcSet) + '"]', typeof n.imageSizes == "string" && (s += '[imagesizes="' + Kt(n.imageSizes) + '"]')) : s += '[href="' + Kt(e) + '"]';
            var f = s;
            switch (t) {
              case "style":
                f = xa(e);
                break;
              case "script":
                f = Ta(e);
            }
            nn.has(f) || (e = p({
              rel: "preload",
              href: t === "image" && n && n.imageSrcSet ? void 0 : e,
              as: t
            }, n), nn.set(f, e), r.querySelector(s) !== null || t === "style" && r.querySelector(Rr(f)) || t === "script" && r.querySelector(Cr(f)) || (t = r.createElement("link"), xt(t, "link", e), pt(t), r.head.appendChild(t)));
          }
        }
        function lb(e, t) {
          Hn.m(e, t);
          var n = Sa;
          if (n && e) {
            var r = t && typeof t.as == "string" ? t.as : "script", s = 'link[rel="modulepreload"][as="' + Kt(r) + '"][href="' + Kt(e) + '"]', f = s;
            switch (r) {
              case "audioworklet":
              case "paintworklet":
              case "serviceworker":
              case "sharedworker":
              case "worker":
              case "script":
                f = Ta(e);
            }
            if (!nn.has(f) && (e = p({
              rel: "modulepreload",
              href: e
            }, t), nn.set(f, e), n.querySelector(s) === null)) {
              switch (r) {
                case "audioworklet":
                case "paintworklet":
                case "serviceworker":
                case "sharedworker":
                case "worker":
                case "script":
                  if (n.querySelector(Cr(f))) return;
              }
              r = n.createElement("link"), xt(r, "link", e), pt(r), n.head.appendChild(r);
            }
          }
        }
        function ob(e, t, n) {
          Hn.S(e, t, n);
          var r = Sa;
          if (r && e) {
            var s = Yi(r).hoistableStyles, f = xa(e);
            t = t || "default";
            var v = s.get(f);
            if (!v) {
              var b = {
                loading: 0,
                preload: null
              };
              if (v = r.querySelector(Rr(f))) b.loading = 5;
              else {
                e = p({
                  rel: "stylesheet",
                  href: e,
                  "data-precedence": t
                }, n), (n = nn.get(f)) && Us(e, n);
                var A = v = r.createElement("link");
                pt(A), xt(A, "link", e), A._p = new Promise(function(j, Q) {
                  A.onload = j, A.onerror = Q;
                }), A.addEventListener("load", function() {
                  b.loading |= 1;
                }), A.addEventListener("error", function() {
                  b.loading |= 2;
                }), b.loading |= 4, Wl(v, t, r);
              }
              v = {
                type: "stylesheet",
                instance: v,
                count: 1,
                state: b
              }, s.set(f, v);
            }
          }
        }
        function ub(e, t) {
          Hn.X(e, t);
          var n = Sa;
          if (n && e) {
            var r = Yi(n).hoistableScripts, s = Ta(e), f = r.get(s);
            f || (f = n.querySelector(Cr(s)), f || (e = p({
              src: e,
              async: true
            }, t), (t = nn.get(s)) && js(e, t), f = n.createElement("script"), pt(f), xt(f, "link", e), n.head.appendChild(f)), f = {
              type: "script",
              instance: f,
              count: 1,
              state: null
            }, r.set(s, f));
          }
        }
        function sb(e, t) {
          Hn.M(e, t);
          var n = Sa;
          if (n && e) {
            var r = Yi(n).hoistableScripts, s = Ta(e), f = r.get(s);
            f || (f = n.querySelector(Cr(s)), f || (e = p({
              src: e,
              async: true,
              type: "module"
            }, t), (t = nn.get(s)) && js(e, t), f = n.createElement("script"), pt(f), xt(f, "link", e), n.head.appendChild(f)), f = {
              type: "script",
              instance: f,
              count: 1,
              state: null
            }, r.set(s, f));
          }
        }
        function Tg(e, t, n, r) {
          var s = (s = ce.current) ? Il(s) : null;
          if (!s) throw Error(i(446));
          switch (e) {
            case "meta":
            case "title":
              return null;
            case "style":
              return typeof n.precedence == "string" && typeof n.href == "string" ? (t = xa(n.href), n = Yi(s).hoistableStyles, r = n.get(t), r || (r = {
                type: "style",
                instance: null,
                count: 0,
                state: null
              }, n.set(t, r)), r) : {
                type: "void",
                instance: null,
                count: 0,
                state: null
              };
            case "link":
              if (n.rel === "stylesheet" && typeof n.href == "string" && typeof n.precedence == "string") {
                e = xa(n.href);
                var f = Yi(s).hoistableStyles, v = f.get(e);
                if (v || (s = s.ownerDocument || s, v = {
                  type: "stylesheet",
                  instance: null,
                  count: 0,
                  state: {
                    loading: 0,
                    preload: null
                  }
                }, f.set(e, v), (f = s.querySelector(Rr(e))) && !f._p && (v.instance = f, v.state.loading = 5), nn.has(e) || (n = {
                  rel: "preload",
                  as: "style",
                  href: n.href,
                  crossOrigin: n.crossOrigin,
                  integrity: n.integrity,
                  media: n.media,
                  hrefLang: n.hrefLang,
                  referrerPolicy: n.referrerPolicy
                }, nn.set(e, n), f || cb(s, e, n, v.state))), t && r === null) throw Error(i(528, ""));
                return v;
              }
              if (t && r !== null) throw Error(i(529, ""));
              return null;
            case "script":
              return t = n.async, n = n.src, typeof n == "string" && t && typeof t != "function" && typeof t != "symbol" ? (t = Ta(n), n = Yi(s).hoistableScripts, r = n.get(t), r || (r = {
                type: "script",
                instance: null,
                count: 0,
                state: null
              }, n.set(t, r)), r) : {
                type: "void",
                instance: null,
                count: 0,
                state: null
              };
            default:
              throw Error(i(444, e));
          }
        }
        function xa(e) {
          return 'href="' + Kt(e) + '"';
        }
        function Rr(e) {
          return 'link[rel="stylesheet"][' + e + "]";
        }
        function Ag(e) {
          return p({}, e, {
            "data-precedence": e.precedence,
            precedence: null
          });
        }
        function cb(e, t, n, r) {
          e.querySelector('link[rel="preload"][as="style"][' + t + "]") ? r.loading = 1 : (t = e.createElement("link"), r.preload = t, t.addEventListener("load", function() {
            return r.loading |= 1;
          }), t.addEventListener("error", function() {
            return r.loading |= 2;
          }), xt(t, "link", n), pt(t), e.head.appendChild(t));
        }
        function Ta(e) {
          return '[src="' + Kt(e) + '"]';
        }
        function Cr(e) {
          return "script[async]" + e;
        }
        function Rg(e, t, n) {
          if (t.count++, t.instance === null) switch (t.type) {
            case "style":
              var r = e.querySelector('style[data-href~="' + Kt(n.href) + '"]');
              if (r) return t.instance = r, pt(r), r;
              var s = p({}, n, {
                "data-href": n.href,
                "data-precedence": n.precedence,
                href: null,
                precedence: null
              });
              return r = (e.ownerDocument || e).createElement("style"), pt(r), xt(r, "style", s), Wl(r, n.precedence, e), t.instance = r;
            case "stylesheet":
              s = xa(n.href);
              var f = e.querySelector(Rr(s));
              if (f) return t.state.loading |= 4, t.instance = f, pt(f), f;
              r = Ag(n), (s = nn.get(s)) && Us(r, s), f = (e.ownerDocument || e).createElement("link"), pt(f);
              var v = f;
              return v._p = new Promise(function(b, A) {
                v.onload = b, v.onerror = A;
              }), xt(f, "link", r), t.state.loading |= 4, Wl(f, n.precedence, e), t.instance = f;
            case "script":
              return f = Ta(n.src), (s = e.querySelector(Cr(f))) ? (t.instance = s, pt(s), s) : (r = n, (s = nn.get(f)) && (r = p({}, n), js(r, s)), e = e.ownerDocument || e, s = e.createElement("script"), pt(s), xt(s, "link", r), e.head.appendChild(s), t.instance = s);
            case "void":
              return null;
            default:
              throw Error(i(443, t.type));
          }
          else t.type === "stylesheet" && (t.state.loading & 4) === 0 && (r = t.instance, t.state.loading |= 4, Wl(r, n.precedence, e));
          return t.instance;
        }
        function Wl(e, t, n) {
          for (var r = n.querySelectorAll('link[rel="stylesheet"][data-precedence],style[data-precedence]'), s = r.length ? r[r.length - 1] : null, f = s, v = 0; v < r.length; v++) {
            var b = r[v];
            if (b.dataset.precedence === t) f = b;
            else if (f !== s) break;
          }
          f ? f.parentNode.insertBefore(e, f.nextSibling) : (t = n.nodeType === 9 ? n.head : n, t.insertBefore(e, t.firstChild));
        }
        function Us(e, t) {
          e.crossOrigin == null && (e.crossOrigin = t.crossOrigin), e.referrerPolicy == null && (e.referrerPolicy = t.referrerPolicy), e.title == null && (e.title = t.title);
        }
        function js(e, t) {
          e.crossOrigin == null && (e.crossOrigin = t.crossOrigin), e.referrerPolicy == null && (e.referrerPolicy = t.referrerPolicy), e.integrity == null && (e.integrity = t.integrity);
        }
        var Jl = null;
        function Cg(e, t, n) {
          if (Jl === null) {
            var r = /* @__PURE__ */ new Map(), s = Jl = /* @__PURE__ */ new Map();
            s.set(n, r);
          } else s = Jl, r = s.get(n), r || (r = /* @__PURE__ */ new Map(), s.set(n, r));
          if (r.has(e)) return r;
          for (r.set(e, null), n = n.getElementsByTagName(e), s = 0; s < n.length; s++) {
            var f = n[s];
            if (!(f[Fa] || f[At] || e === "link" && f.getAttribute("rel") === "stylesheet") && f.namespaceURI !== "http://www.w3.org/2000/svg") {
              var v = f.getAttribute(t) || "";
              v = e + v;
              var b = r.get(v);
              b ? b.push(f) : r.set(v, [
                f
              ]);
            }
          }
          return r;
        }
        function Dg(e, t, n) {
          e = e.ownerDocument || e, e.head.insertBefore(n, t === "title" ? e.querySelector("head > title") : null);
        }
        function fb(e, t, n) {
          if (n === 1 || t.itemProp != null) return false;
          switch (e) {
            case "meta":
            case "title":
              return true;
            case "style":
              if (typeof t.precedence != "string" || typeof t.href != "string" || t.href === "") break;
              return true;
            case "link":
              if (typeof t.rel != "string" || typeof t.href != "string" || t.href === "" || t.onLoad || t.onError) break;
              switch (t.rel) {
                case "stylesheet":
                  return e = t.disabled, typeof t.precedence == "string" && e == null;
                default:
                  return true;
              }
            case "script":
              if (t.async && typeof t.async != "function" && typeof t.async != "symbol" && !t.onLoad && !t.onError && t.src && typeof t.src == "string") return true;
          }
          return false;
        }
        function Ng(e) {
          return !(e.type === "stylesheet" && (e.state.loading & 3) === 0);
        }
        var Dr = null;
        function db() {
        }
        function hb(e, t, n) {
          if (Dr === null) throw Error(i(475));
          var r = Dr;
          if (t.type === "stylesheet" && (typeof n.media != "string" || matchMedia(n.media).matches !== false) && (t.state.loading & 4) === 0) {
            if (t.instance === null) {
              var s = xa(n.href), f = e.querySelector(Rr(s));
              if (f) {
                e = f._p, e !== null && typeof e == "object" && typeof e.then == "function" && (r.count++, r = eo.bind(r), e.then(r, r)), t.state.loading |= 4, t.instance = f, pt(f);
                return;
              }
              f = e.ownerDocument || e, n = Ag(n), (s = nn.get(s)) && Us(n, s), f = f.createElement("link"), pt(f);
              var v = f;
              v._p = new Promise(function(b, A) {
                v.onload = b, v.onerror = A;
              }), xt(f, "link", n), t.instance = f;
            }
            r.stylesheets === null && (r.stylesheets = /* @__PURE__ */ new Map()), r.stylesheets.set(t, e), (e = t.state.preload) && (t.state.loading & 3) === 0 && (r.count++, t = eo.bind(r), e.addEventListener("load", t), e.addEventListener("error", t));
          }
        }
        function gb() {
          if (Dr === null) throw Error(i(475));
          var e = Dr;
          return e.stylesheets && e.count === 0 && Bs(e, e.stylesheets), 0 < e.count ? function(t) {
            var n = setTimeout(function() {
              if (e.stylesheets && Bs(e, e.stylesheets), e.unsuspend) {
                var r = e.unsuspend;
                e.unsuspend = null, r();
              }
            }, 6e4);
            return e.unsuspend = t, function() {
              e.unsuspend = null, clearTimeout(n);
            };
          } : null;
        }
        function eo() {
          if (this.count--, this.count === 0) {
            if (this.stylesheets) Bs(this, this.stylesheets);
            else if (this.unsuspend) {
              var e = this.unsuspend;
              this.unsuspend = null, e();
            }
          }
        }
        var to = null;
        function Bs(e, t) {
          e.stylesheets = null, e.unsuspend !== null && (e.count++, to = /* @__PURE__ */ new Map(), t.forEach(mb, e), to = null, eo.call(e));
        }
        function mb(e, t) {
          if (!(t.state.loading & 4)) {
            var n = to.get(e);
            if (n) var r = n.get(null);
            else {
              n = /* @__PURE__ */ new Map(), to.set(e, n);
              for (var s = e.querySelectorAll("link[data-precedence],style[data-precedence]"), f = 0; f < s.length; f++) {
                var v = s[f];
                (v.nodeName === "LINK" || v.getAttribute("media") !== "not all") && (n.set(v.dataset.precedence, v), r = v);
              }
              r && n.set(null, r);
            }
            s = t.instance, v = s.getAttribute("data-precedence"), f = n.get(v) || r, f === r && n.set(null, s), n.set(v, s), this.count++, r = eo.bind(this), s.addEventListener("load", r), s.addEventListener("error", r), f ? f.parentNode.insertBefore(s, f.nextSibling) : (e = e.nodeType === 9 ? e.head : e, e.insertBefore(s, e.firstChild)), t.state.loading |= 4;
          }
        }
        var Nr = {
          $$typeof: I,
          Provider: null,
          Consumer: null,
          _currentValue: $,
          _currentValue2: $,
          _threadCount: 0
        };
        function vb(e, t, n, r, s, f, v, b) {
          this.tag = 1, this.containerInfo = e, this.pingCache = this.current = this.pendingChildren = null, this.timeoutHandle = -1, this.callbackNode = this.next = this.pendingContext = this.context = this.cancelPendingCommit = null, this.callbackPriority = 0, this.expirationTimes = Lo(-1), this.entangledLanes = this.shellSuspendCounter = this.errorRecoveryDisabledLanes = this.expiredLanes = this.warmLanes = this.pingedLanes = this.suspendedLanes = this.pendingLanes = 0, this.entanglements = Lo(0), this.hiddenUpdates = Lo(null), this.identifierPrefix = r, this.onUncaughtError = s, this.onCaughtError = f, this.onRecoverableError = v, this.pooledCache = null, this.pooledCacheLanes = 0, this.formState = b, this.incompleteTransitions = /* @__PURE__ */ new Map();
        }
        function Og(e, t, n, r, s, f, v, b, A, j, Q, W) {
          return e = new vb(e, t, n, v, b, A, j, W), t = 1, f === true && (t |= 24), f = jt(3, null, null, t), e.current = f, f.stateNode = e, t = _u(), t.refCount++, e.pooledCache = t, t.refCount++, f.memoizedState = {
            element: r,
            isDehydrated: n,
            cache: t
          }, xu(f), e;
        }
        function zg(e) {
          return e ? (e = na, e) : na;
        }
        function kg(e, t, n, r, s, f) {
          s = zg(s), r.context === null ? r.context = s : r.pendingContext = s, r = Wn(t), r.payload = {
            element: n
          }, f = f === void 0 ? null : f, f !== null && (r.callback = f), n = Jn(e, r, t), n !== null && (Vt(n, e, t), lr(n, e, t));
        }
        function Mg(e, t) {
          if (e = e.memoizedState, e !== null && e.dehydrated !== null) {
            var n = e.retryLane;
            e.retryLane = n !== 0 && n < t ? n : t;
          }
        }
        function Hs(e, t) {
          Mg(e, t), (e = e.alternate) && Mg(e, t);
        }
        function Lg(e) {
          if (e.tag === 13) {
            var t = ta(e, 67108864);
            t !== null && Vt(t, e, 67108864), Hs(e, 67108864);
          }
        }
        var no = true;
        function pb(e, t, n, r) {
          var s = D.T;
          D.T = null;
          var f = X.p;
          try {
            X.p = 2, Fs(e, t, n, r);
          } finally {
            X.p = f, D.T = s;
          }
        }
        function yb(e, t, n, r) {
          var s = D.T;
          D.T = null;
          var f = X.p;
          try {
            X.p = 8, Fs(e, t, n, r);
          } finally {
            X.p = f, D.T = s;
          }
        }
        function Fs(e, t, n, r) {
          if (no) {
            var s = qs(r);
            if (s === null) Cs(e, t, r, io, n), Ug(e, r);
            else if (_b(s, e, t, n, r)) r.stopPropagation();
            else if (Ug(e, r), t & 4 && -1 < bb.indexOf(e)) {
              for (; s !== null; ) {
                var f = Vi(s);
                if (f !== null) switch (f.tag) {
                  case 3:
                    if (f = f.stateNode, f.current.memoizedState.isDehydrated) {
                      var v = _i(f.pendingLanes);
                      if (v !== 0) {
                        var b = f;
                        for (b.pendingLanes |= 2, b.entangledLanes |= 2; v; ) {
                          var A = 1 << 31 - wt(v);
                          b.entanglements[1] |= A, v &= ~A;
                        }
                        Sn(f), (He & 6) === 0 && (Hl = nt() + 500, Sr(0));
                      }
                    }
                    break;
                  case 13:
                    b = ta(f, 2), b !== null && Vt(b, f, 2), ql(), Hs(f, 2);
                }
                if (f = qs(r), f === null && Cs(e, t, r, io, n), f === s) break;
                s = f;
              }
              s !== null && r.stopPropagation();
            } else Cs(e, t, r, null, n);
          }
        }
        function qs(e) {
          return e = Zo(e), Vs(e);
        }
        var io = null;
        function Vs(e) {
          if (io = null, e = qi(e), e !== null) {
            var t = c(e);
            if (t === null) e = null;
            else {
              var n = t.tag;
              if (n === 13) {
                if (e = d(t), e !== null) return e;
                e = null;
              } else if (n === 3) {
                if (t.stateNode.current.memoizedState.isDehydrated) return t.tag === 3 ? t.stateNode.containerInfo : null;
                e = null;
              } else t !== e && (e = null);
            }
          }
          return io = e, null;
        }
        function Gg(e) {
          switch (e) {
            case "beforetoggle":
            case "cancel":
            case "click":
            case "close":
            case "contextmenu":
            case "copy":
            case "cut":
            case "auxclick":
            case "dblclick":
            case "dragend":
            case "dragstart":
            case "drop":
            case "focusin":
            case "focusout":
            case "input":
            case "invalid":
            case "keydown":
            case "keypress":
            case "keyup":
            case "mousedown":
            case "mouseup":
            case "paste":
            case "pause":
            case "play":
            case "pointercancel":
            case "pointerdown":
            case "pointerup":
            case "ratechange":
            case "reset":
            case "resize":
            case "seeked":
            case "submit":
            case "toggle":
            case "touchcancel":
            case "touchend":
            case "touchstart":
            case "volumechange":
            case "change":
            case "selectionchange":
            case "textInput":
            case "compositionstart":
            case "compositionend":
            case "compositionupdate":
            case "beforeblur":
            case "afterblur":
            case "beforeinput":
            case "blur":
            case "fullscreenchange":
            case "focus":
            case "hashchange":
            case "popstate":
            case "select":
            case "selectstart":
              return 2;
            case "drag":
            case "dragenter":
            case "dragexit":
            case "dragleave":
            case "dragover":
            case "mousemove":
            case "mouseout":
            case "mouseover":
            case "pointermove":
            case "pointerout":
            case "pointerover":
            case "scroll":
            case "touchmove":
            case "wheel":
            case "mouseenter":
            case "mouseleave":
            case "pointerenter":
            case "pointerleave":
              return 8;
            case "message":
              switch (Te()) {
                case Ke:
                  return 2;
                case un:
                  return 8;
                case sn:
                case $t:
                  return 32;
                case Xn:
                  return 268435456;
                default:
                  return 32;
              }
            default:
              return 32;
          }
        }
        var Ys = false, di = null, hi = null, gi = null, Or = /* @__PURE__ */ new Map(), zr = /* @__PURE__ */ new Map(), mi = [], bb = "mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset".split(" ");
        function Ug(e, t) {
          switch (e) {
            case "focusin":
            case "focusout":
              di = null;
              break;
            case "dragenter":
            case "dragleave":
              hi = null;
              break;
            case "mouseover":
            case "mouseout":
              gi = null;
              break;
            case "pointerover":
            case "pointerout":
              Or.delete(t.pointerId);
              break;
            case "gotpointercapture":
            case "lostpointercapture":
              zr.delete(t.pointerId);
          }
        }
        function kr(e, t, n, r, s, f) {
          return e === null || e.nativeEvent !== f ? (e = {
            blockedOn: t,
            domEventName: n,
            eventSystemFlags: r,
            nativeEvent: f,
            targetContainers: [
              s
            ]
          }, t !== null && (t = Vi(t), t !== null && Lg(t)), e) : (e.eventSystemFlags |= r, t = e.targetContainers, s !== null && t.indexOf(s) === -1 && t.push(s), e);
        }
        function _b(e, t, n, r, s) {
          switch (t) {
            case "focusin":
              return di = kr(di, e, t, n, r, s), true;
            case "dragenter":
              return hi = kr(hi, e, t, n, r, s), true;
            case "mouseover":
              return gi = kr(gi, e, t, n, r, s), true;
            case "pointerover":
              var f = s.pointerId;
              return Or.set(f, kr(Or.get(f) || null, e, t, n, r, s)), true;
            case "gotpointercapture":
              return f = s.pointerId, zr.set(f, kr(zr.get(f) || null, e, t, n, r, s)), true;
          }
          return false;
        }
        function jg(e) {
          var t = qi(e.target);
          if (t !== null) {
            var n = c(t);
            if (n !== null) {
              if (t = n.tag, t === 13) {
                if (t = d(n), t !== null) {
                  e.blockedOn = t, hp(e.priority, function() {
                    if (n.tag === 13) {
                      var r = qt();
                      r = Go(r);
                      var s = ta(n, r);
                      s !== null && Vt(s, n, r), Hs(n, r);
                    }
                  });
                  return;
                }
              } else if (t === 3 && n.stateNode.current.memoizedState.isDehydrated) {
                e.blockedOn = n.tag === 3 ? n.stateNode.containerInfo : null;
                return;
              }
            }
          }
          e.blockedOn = null;
        }
        function ao(e) {
          if (e.blockedOn !== null) return false;
          for (var t = e.targetContainers; 0 < t.length; ) {
            var n = qs(e.nativeEvent);
            if (n === null) {
              n = e.nativeEvent;
              var r = new n.constructor(n.type, n);
              Xo = r, n.target.dispatchEvent(r), Xo = null;
            } else return t = Vi(n), t !== null && Lg(t), e.blockedOn = n, false;
            t.shift();
          }
          return true;
        }
        function Bg(e, t, n) {
          ao(e) && n.delete(t);
        }
        function wb() {
          Ys = false, di !== null && ao(di) && (di = null), hi !== null && ao(hi) && (hi = null), gi !== null && ao(gi) && (gi = null), Or.forEach(Bg), zr.forEach(Bg);
        }
        function ro(e, t) {
          e.blockedOn === t && (e.blockedOn = null, Ys || (Ys = true, l.unstable_scheduleCallback(l.unstable_NormalPriority, wb)));
        }
        var lo = null;
        function Hg(e) {
          lo !== e && (lo = e, l.unstable_scheduleCallback(l.unstable_NormalPriority, function() {
            lo === e && (lo = null);
            for (var t = 0; t < e.length; t += 3) {
              var n = e[t], r = e[t + 1], s = e[t + 2];
              if (typeof r != "function") {
                if (Vs(r || n) === null) continue;
                break;
              }
              var f = Vi(n);
              f !== null && (e.splice(t, 3), t -= 3, Vu(f, {
                pending: true,
                data: s,
                method: n.method,
                action: r
              }, r, s));
            }
          }));
        }
        function Mr(e) {
          function t(A) {
            return ro(A, e);
          }
          di !== null && ro(di, e), hi !== null && ro(hi, e), gi !== null && ro(gi, e), Or.forEach(t), zr.forEach(t);
          for (var n = 0; n < mi.length; n++) {
            var r = mi[n];
            r.blockedOn === e && (r.blockedOn = null);
          }
          for (; 0 < mi.length && (n = mi[0], n.blockedOn === null); ) jg(n), n.blockedOn === null && mi.shift();
          if (n = (e.ownerDocument || e).$$reactFormReplay, n != null) for (r = 0; r < n.length; r += 3) {
            var s = n[r], f = n[r + 1], v = s[zt] || null;
            if (typeof f == "function") v || Hg(n);
            else if (v) {
              var b = null;
              if (f && f.hasAttribute("formAction")) {
                if (s = f, v = f[zt] || null) b = v.formAction;
                else if (Vs(s) !== null) continue;
              } else b = v.action;
              typeof b == "function" ? n[r + 1] = b : (n.splice(r, 3), r -= 3), Hg(n);
            }
          }
        }
        function $s(e) {
          this._internalRoot = e;
        }
        oo.prototype.render = $s.prototype.render = function(e) {
          var t = this._internalRoot;
          if (t === null) throw Error(i(409));
          var n = t.current, r = qt();
          kg(n, r, e, t, null, null);
        }, oo.prototype.unmount = $s.prototype.unmount = function() {
          var e = this._internalRoot;
          if (e !== null) {
            this._internalRoot = null;
            var t = e.containerInfo;
            kg(e.current, 2, null, e, null, null), ql(), t[Fi] = null;
          }
        };
        function oo(e) {
          this._internalRoot = e;
        }
        oo.prototype.unstable_scheduleHydration = function(e) {
          if (e) {
            var t = nf();
            e = {
              blockedOn: null,
              target: e,
              priority: t
            };
            for (var n = 0; n < mi.length && t !== 0 && t < mi[n].priority; n++) ;
            mi.splice(n, 0, e), n === 0 && jg(e);
          }
        };
        var Fg = a.version;
        if (Fg !== "19.1.0") throw Error(i(527, Fg, "19.1.0"));
        X.findDOMNode = function(e) {
          var t = e._reactInternals;
          if (t === void 0) throw typeof e.render == "function" ? Error(i(188)) : (e = Object.keys(e).join(","), Error(i(268, e)));
          return e = m(t), e = e !== null ? g(e) : null, e = e === null ? null : e.stateNode, e;
        };
        var Eb = {
          bundleType: 0,
          version: "19.1.0",
          rendererPackageName: "react-dom",
          currentDispatcherRef: D,
          reconcilerVersion: "19.1.0"
        };
        if (typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ < "u") {
          var uo = __REACT_DEVTOOLS_GLOBAL_HOOK__;
          if (!uo.isDisabled && uo.supportsFiber) try {
            Je = uo.inject(Eb), Ve = uo;
          } catch {
          }
        }
        return Gr.createRoot = function(e, t) {
          if (!u(e)) throw Error(i(299));
          var n = false, r = "", s = ih, f = ah, v = rh, b = null;
          return t != null && (t.unstable_strictMode === true && (n = true), t.identifierPrefix !== void 0 && (r = t.identifierPrefix), t.onUncaughtError !== void 0 && (s = t.onUncaughtError), t.onCaughtError !== void 0 && (f = t.onCaughtError), t.onRecoverableError !== void 0 && (v = t.onRecoverableError), t.unstable_transitionCallbacks !== void 0 && (b = t.unstable_transitionCallbacks)), t = Og(e, 1, false, null, null, n, r, s, f, v, b, null), e[Fi] = t.current, Rs(e), new $s(t);
        }, Gr.hydrateRoot = function(e, t, n) {
          if (!u(e)) throw Error(i(299));
          var r = false, s = "", f = ih, v = ah, b = rh, A = null, j = null;
          return n != null && (n.unstable_strictMode === true && (r = true), n.identifierPrefix !== void 0 && (s = n.identifierPrefix), n.onUncaughtError !== void 0 && (f = n.onUncaughtError), n.onCaughtError !== void 0 && (v = n.onCaughtError), n.onRecoverableError !== void 0 && (b = n.onRecoverableError), n.unstable_transitionCallbacks !== void 0 && (A = n.unstable_transitionCallbacks), n.formState !== void 0 && (j = n.formState)), t = Og(e, 1, true, t, n ?? null, r, s, f, v, b, A, j), t.context = zg(null), n = t.current, r = qt(), r = Go(r), s = Wn(r), s.callback = null, Jn(n, s, r), n = r, t.current.lanes = n, Ha(t, n), Sn(t), e[Fi] = t.current, Rs(e), new oo(t);
        }, Gr.version = "19.1.0", Gr;
      }
      var Ig;
      function zb() {
        if (Ig) return Qs.exports;
        Ig = 1;
        function l() {
          if (!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__ > "u" || typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE != "function")) try {
            __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(l);
          } catch (a) {
            console.error(a);
          }
        }
        return l(), Qs.exports = Ob(), Qs.exports;
      }
      var kb = zb();
      const Mb = "/assets/gossip_sim_wasm_bg-Bt8RWnX7.wasm", Lb = async (l = {}, a) => {
        let o;
        if (a.startsWith("data:")) {
          const i = a.replace(/^data:.*?base64,/, "");
          let u;
          if (typeof Buffer == "function" && typeof Buffer.from == "function") u = Buffer.from(i, "base64");
          else if (typeof atob == "function") {
            const c = atob(i);
            u = new Uint8Array(c.length);
            for (let d = 0; d < c.length; d++) u[d] = c.charCodeAt(d);
          } else throw new Error("Cannot decode base64-encoded data URL");
          o = await WebAssembly.instantiate(u, l);
        } else {
          const i = await fetch(a), u = i.headers.get("Content-Type") || "";
          if ("instantiateStreaming" in WebAssembly && u.startsWith("application/wasm")) o = await WebAssembly.instantiateStreaming(i, l);
          else {
            const c = await i.arrayBuffer();
            o = await WebAssembly.instantiate(c, l);
          }
        }
        return o.instance.exports;
      };
      let Ie;
      function Gb(l) {
        Ie = l;
      }
      function qe(l, a) {
        try {
          return l.apply(this, a);
        } catch (o) {
          let i = function() {
            try {
              return o instanceof Error ? `${o.message}

Stack:
${o.stack}` : o.toString();
            } catch {
              return "<failed to stringify thrown value>";
            }
          }();
          throw console.error("wasm-bindgen: imported JS function that was not marked as `catch` threw an error:", i), o;
        }
      }
      function Zr(l) {
        const a = Ie.__externref_table_alloc();
        return Ie.__wbindgen_export_2.set(a, l), a;
      }
      function Ub(l, a) {
        try {
          return l.apply(this, a);
        } catch (o) {
          const i = Zr(o);
          Ie.__wbindgen_exn_store(i);
        }
      }
      let Aa = null;
      function pn() {
        return (Aa === null || Aa.buffer.detached === true || Aa.buffer.detached === void 0 && Aa.buffer !== Ie.memory.buffer) && (Aa = new DataView(Ie.memory.buffer)), Aa;
      }
      function Ro(l, a) {
        l = l >>> 0;
        const o = pn(), i = [];
        for (let u = l; u < l + 4 * a; u += 4) i.push(Ie.__wbindgen_export_2.get(o.getUint32(u, true)));
        return Ie.__externref_drop_slice(l, a), i;
      }
      const jb = typeof TextDecoder > "u" ? (0, module.require)("util").TextDecoder : TextDecoder;
      let Pm = new jb("utf-8", {
        ignoreBOM: true,
        fatal: true
      });
      Pm.decode();
      let so = null;
      function wo() {
        return (so === null || so.byteLength === 0) && (so = new Uint8Array(Ie.memory.buffer)), so;
      }
      function Qr(l, a) {
        return l = l >>> 0, Pm.decode(wo().subarray(l, l + a));
      }
      function Tn(l) {
        if (typeof l != "boolean") throw new Error(`expected a boolean argument, found ${typeof l}`);
      }
      function Co(l) {
        if (typeof l != "number") throw new Error(`expected a number argument, found ${typeof l}`);
      }
      let $r = 0;
      const Bb = typeof TextEncoder > "u" ? (0, module.require)("util").TextEncoder : TextEncoder;
      let Eo = new Bb("utf-8");
      const Hb = typeof Eo.encodeInto == "function" ? function(l, a) {
        return Eo.encodeInto(l, a);
      } : function(l, a) {
        const o = Eo.encode(l);
        return a.set(o), {
          read: l.length,
          written: o.length
        };
      };
      function Uc(l, a, o) {
        if (typeof l != "string") throw new Error(`expected a string argument, found ${typeof l}`);
        if (o === void 0) {
          const h = Eo.encode(l), m = a(h.length, 1) >>> 0;
          return wo().subarray(m, m + h.length).set(h), $r = h.length, m;
        }
        let i = l.length, u = a(i, 1) >>> 0;
        const c = wo();
        let d = 0;
        for (; d < i; d++) {
          const h = l.charCodeAt(d);
          if (h > 127) break;
          c[u + d] = h;
        }
        if (d !== i) {
          d !== 0 && (l = l.slice(d)), u = o(u, i, i = d + l.length * 3, 1) >>> 0;
          const h = wo().subarray(u + d, u + i), m = Hb(l, h);
          if (m.read !== l.length) throw new Error("failed to pass whole string");
          d += m.written, u = o(u, i, d, 1) >>> 0;
        }
        return $r = d, u;
      }
      function vn(l) {
        return l == null;
      }
      function Fb(l) {
        if (typeof l != "bigint") throw new Error(`expected a bigint argument, found ${typeof l}`);
      }
      function Ec(l) {
        const a = typeof l;
        if (a == "number" || a == "boolean" || l == null) return `${l}`;
        if (a == "string") return `"${l}"`;
        if (a == "symbol") {
          const u = l.description;
          return u == null ? "Symbol" : `Symbol(${u})`;
        }
        if (a == "function") {
          const u = l.name;
          return typeof u == "string" && u.length > 0 ? `Function(${u})` : "Function";
        }
        if (Array.isArray(l)) {
          const u = l.length;
          let c = "[";
          u > 0 && (c += Ec(l[0]));
          for (let d = 1; d < u; d++) c += ", " + Ec(l[d]);
          return c += "]", c;
        }
        const o = /\[object ([^\]]+)\]/.exec(toString.call(l));
        let i;
        if (o && o.length > 1) i = o[1];
        else return toString.call(l);
        if (i == "Object") try {
          return "Object(" + JSON.stringify(l) + ")";
        } catch {
          return "Object";
        }
        return l instanceof Error ? `${l.name}: ${l.message}
${l.stack}` : i;
      }
      function qb() {
        return Ie.default_config();
      }
      function Wg(l) {
        const a = Ie.__wbindgen_export_2.get(l);
        return Ie.__externref_table_dealloc(l), a;
      }
      function Vb(l) {
        const a = Ie.run_simulation(l);
        if (a[2]) throw Wg(a[1]);
        return Wg(a[0]);
      }
      function Yb() {
        return qe(function(l) {
          return l.buffer;
        }, arguments);
      }
      function $b() {
        return Ub(function(l, a) {
          return l.call(a);
        }, arguments);
      }
      function Xb() {
        return qe(function(l, a) {
          var o = Ro(l, a).slice();
          Ie.__wbindgen_free(l, a * 4, 4), console.debug(...o);
        }, arguments);
      }
      function Zb() {
        return qe(function(l) {
          return Object.entries(l);
        }, arguments);
      }
      function Qb() {
        return qe(function(l, a) {
          let o, i;
          try {
            o = l, i = a, console.error(Qr(l, a));
          } finally {
            Ie.__wbindgen_free(o, i, 1);
          }
        }, arguments);
      }
      function Kb() {
        return qe(function(l, a) {
          var o = Ro(l, a).slice();
          Ie.__wbindgen_free(l, a * 4, 4), console.error(...o);
        }, arguments);
      }
      function Pb() {
        return qe(function(l, a) {
          return l[a >>> 0];
        }, arguments);
      }
      function Ib() {
        return qe(function(l, a) {
          return l[a];
        }, arguments);
      }
      function Wb() {
        return qe(function(l) {
          let a;
          try {
            a = l instanceof ArrayBuffer;
          } catch {
            a = false;
          }
          const o = a;
          return Tn(o), o;
        }, arguments);
      }
      function Jb() {
        return qe(function(l) {
          let a;
          try {
            a = l instanceof Uint8Array;
          } catch {
            a = false;
          }
          const o = a;
          return Tn(o), o;
        }, arguments);
      }
      function e_() {
        return qe(function(l) {
          const a = Number.isSafeInteger(l);
          return Tn(a), a;
        }, arguments);
      }
      function t_() {
        return qe(function(l) {
          const a = l.length;
          return Co(a), a;
        }, arguments);
      }
      function n_() {
        return qe(function(l) {
          const a = l.length;
          return Co(a), a;
        }, arguments);
      }
      function i_() {
        return qe(function(l, a) {
          var o = Ro(l, a).slice();
          Ie.__wbindgen_free(l, a * 4, 4), console.log(...o);
        }, arguments);
      }
      function a_() {
        return qe(function() {
          return new Object();
        }, arguments);
      }
      function r_() {
        return qe(function() {
          return new Array();
        }, arguments);
      }
      function l_() {
        return qe(function() {
          return new Error();
        }, arguments);
      }
      function o_() {
        return qe(function(l) {
          return new Uint8Array(l);
        }, arguments);
      }
      function u_() {
        return qe(function(l, a) {
          return new Function(Qr(l, a));
        }, arguments);
      }
      function s_() {
        return qe(function(l) {
          return l.now();
        }, arguments);
      }
      function c_() {
        return qe(function(l) {
          return l.performance;
        }, arguments);
      }
      function f_() {
        return qe(function(l, a, o) {
          l[a >>> 0] = o;
        }, arguments);
      }
      function d_() {
        return qe(function(l, a, o) {
          l[a] = o;
        }, arguments);
      }
      function h_() {
        return qe(function(l, a, o) {
          l.set(a, o >>> 0);
        }, arguments);
      }
      function g_() {
        return qe(function(l, a) {
          const o = a.stack, i = Uc(o, Ie.__wbindgen_malloc, Ie.__wbindgen_realloc), u = $r;
          pn().setInt32(l + 4 * 1, u, true), pn().setInt32(l + 4 * 0, i, true);
        }, arguments);
      }
      function m_() {
        return qe(function() {
          const l = typeof global > "u" ? null : global;
          return vn(l) ? 0 : Zr(l);
        }, arguments);
      }
      function v_() {
        return qe(function() {
          const l = typeof globalThis > "u" ? null : globalThis;
          return vn(l) ? 0 : Zr(l);
        }, arguments);
      }
      function p_() {
        return qe(function() {
          const l = typeof self > "u" ? null : self;
          return vn(l) ? 0 : Zr(l);
        }, arguments);
      }
      function y_() {
        return qe(function() {
          const l = typeof window > "u" ? null : window;
          return vn(l) ? 0 : Zr(l);
        }, arguments);
      }
      function b_() {
        return qe(function(l, a) {
          var o = Ro(l, a).slice();
          Ie.__wbindgen_free(l, a * 4, 4), console.warn(...o);
        }, arguments);
      }
      function __(l) {
        return +l;
      }
      function w_(l) {
        return BigInt.asUintN(64, l);
      }
      function E_(l, a) {
        const o = a, i = typeof o == "bigint" ? o : void 0;
        vn(i) || Fb(i), pn().setBigInt64(l + 8 * 1, vn(i) ? BigInt(0) : i, true), pn().setInt32(l + 4 * 0, !vn(i), true);
      }
      function S_(l) {
        const a = l, o = typeof a == "boolean" ? a ? 1 : 0 : 2;
        return Co(o), o;
      }
      function x_(l, a) {
        const o = Ec(a), i = Uc(o, Ie.__wbindgen_malloc, Ie.__wbindgen_realloc), u = $r;
        pn().setInt32(l + 4 * 1, u, true), pn().setInt32(l + 4 * 0, i, true);
      }
      function T_(l, a) {
        return new Error(Qr(l, a));
      }
      function A_(l, a) {
        const o = l in a;
        return Tn(o), o;
      }
      function R_() {
        const l = Ie.__wbindgen_export_2, a = l.grow(4);
        l.set(0, void 0), l.set(a + 0, void 0), l.set(a + 1, null), l.set(a + 2, true), l.set(a + 3, false);
      }
      function C_(l) {
        const a = typeof l == "bigint";
        return Tn(a), a;
      }
      function D_(l) {
        const a = l, o = typeof a == "object" && a !== null;
        return Tn(o), o;
      }
      function N_(l) {
        const a = typeof l == "string";
        return Tn(a), a;
      }
      function O_(l) {
        const a = l === void 0;
        return Tn(a), a;
      }
      function z_(l, a) {
        const o = l === a;
        return Tn(o), o;
      }
      function k_(l, a) {
        const o = l == a;
        return Tn(o), o;
      }
      function M_() {
        return Ie.memory;
      }
      function L_(l, a) {
        const o = a, i = typeof o == "number" ? o : void 0;
        vn(i) || Co(i), pn().setFloat64(l + 8 * 1, vn(i) ? 0 : i, true), pn().setInt32(l + 4 * 0, !vn(i), true);
      }
      function G_(l) {
        return l;
      }
      function U_(l, a) {
        const o = a, i = typeof o == "string" ? o : void 0;
        var u = vn(i) ? 0 : Uc(i, Ie.__wbindgen_malloc, Ie.__wbindgen_realloc), c = $r;
        pn().setInt32(l + 4 * 1, c, true), pn().setInt32(l + 4 * 0, u, true);
      }
      function j_(l, a) {
        return Qr(l, a);
      }
      function B_(l, a) {
        throw new Error(Qr(l, a));
      }
      URL = globalThis.URL;
      const Yt = await Lb({
        "./gossip_sim_wasm_bg.js": {
          __wbindgen_error_new: T_,
          __wbindgen_is_undefined: O_,
          __wbindgen_as_number: __,
          __wbindgen_in: A_,
          __wbindgen_string_get: U_,
          __wbindgen_is_bigint: C_,
          __wbindgen_is_object: D_,
          __wbindgen_is_string: N_,
          __wbindgen_jsval_eq: z_,
          __wbindgen_bigint_from_u64: w_,
          __wbindgen_number_get: L_,
          __wbindgen_boolean_get: S_,
          __wbindgen_number_new: G_,
          __wbindgen_string_new: j_,
          __wbindgen_jsval_loose_eq: k_,
          __wbg_getwithrefkey_1dc361bd10053bfe: Ib,
          __wbg_set_3f1d0b984ed272ed: d_,
          __wbg_error_7534b8e9a36f1ab4: Qb,
          __wbg_new_8a6f238a6ece86ea: l_,
          __wbg_stack_0ed75d68575b0f3c: g_,
          __wbg_debug_55137df391ebfd29: Xb,
          __wbg_error_91947ba14c44e1c9: Kb,
          __wbg_log_e51ef223c244b133: i_,
          __wbg_warn_479b8bbb8337357b: b_,
          __wbg_performance_7a3ffd0b17f663ad: c_,
          __wbg_now_2c95c9de01293173: s_,
          __wbg_new_78feb108b6472713: r_,
          __wbg_get_b9b93047fe3cf45b: Pb,
          __wbg_set_37837023f3d740e8: f_,
          __wbg_length_e2d2a49132c1b256: n_,
          __wbg_instanceof_ArrayBuffer_e14585432e3737fc: Wb,
          __wbg_newnoargs_105ed471475aaf50: u_,
          __wbg_call_672a4d21634d4a24: $b,
          __wbg_isSafeInteger_343e2beeeece1bb0: e_,
          __wbg_entries_3265d4158b33e5dc: Zb,
          __wbg_new_405e22f390576ce2: a_,
          __wbg_static_accessor_GLOBAL_THIS_56578be7e9f832b0: v_,
          __wbg_static_accessor_SELF_37c5d418e4bf5819: p_,
          __wbg_static_accessor_WINDOW_5de37043a91a9c40: y_,
          __wbg_static_accessor_GLOBAL_88a902d13a557d07: m_,
          __wbg_instanceof_Uint8Array_17156bcf118086a9: Jb,
          __wbg_new_a12002a7f91c75be: o_,
          __wbg_length_a446193dc22c12f8: t_,
          __wbg_set_65595bdd868b3009: h_,
          __wbg_buffer_609cc3eee51ed158: Yb,
          __wbindgen_debug_string: x_,
          __wbindgen_bigint_get_as_i64: E_,
          __wbindgen_throw: B_,
          __wbindgen_memory: M_,
          __wbindgen_init_externref_table: R_
        }
      }, Mb), H_ = Yt.memory, F_ = Yt.start, q_ = Yt.default_config, V_ = Yt.run_simulation, Y_ = Yt.__wbindgen_exn_store, $_ = Yt.__externref_table_alloc, X_ = Yt.__wbindgen_export_2, Z_ = Yt.__externref_drop_slice, Q_ = Yt.__wbindgen_free, K_ = Yt.__wbindgen_malloc, P_ = Yt.__wbindgen_realloc, I_ = Yt.__externref_table_dealloc, Im = Yt.__wbindgen_start, W_ = Object.freeze(Object.defineProperty({
        __proto__: null,
        __externref_drop_slice: Z_,
        __externref_table_alloc: $_,
        __externref_table_dealloc: I_,
        __wbindgen_exn_store: Y_,
        __wbindgen_export_2: X_,
        __wbindgen_free: Q_,
        __wbindgen_malloc: K_,
        __wbindgen_realloc: P_,
        __wbindgen_start: Im,
        default_config: q_,
        memory: H_,
        run_simulation: V_,
        start: F_
      }, Symbol.toStringTag, {
        value: "Module"
      }));
      Gb(W_);
      Im();
      function Wm(l) {
        var a, o, i = "";
        if (typeof l == "string" || typeof l == "number") i += l;
        else if (typeof l == "object") if (Array.isArray(l)) {
          var u = l.length;
          for (a = 0; a < u; a++) l[a] && (o = Wm(l[a])) && (i && (i += " "), i += o);
        } else for (o in l) l[o] && (i && (i += " "), i += o);
        return i;
      }
      function Jm() {
        for (var l, a, o = 0, i = "", u = arguments.length; o < u; o++) (l = arguments[o]) && (a = Wm(l)) && (i && (i += " "), i += a);
        return i;
      }
      const jc = "-", J_ = (l) => {
        const a = t0(l), { conflictingClassGroups: o, conflictingClassGroupModifiers: i } = l;
        return {
          getClassGroupId: (d) => {
            const h = d.split(jc);
            return h[0] === "" && h.length !== 1 && h.shift(), ev(h, a) || e0(d);
          },
          getConflictingClassGroupIds: (d, h) => {
            const m = o[d] || [];
            return h && i[d] ? [
              ...m,
              ...i[d]
            ] : m;
          }
        };
      }, ev = (l, a) => {
        var _a;
        if (l.length === 0) return a.classGroupId;
        const o = l[0], i = a.nextPart.get(o), u = i ? ev(l.slice(1), i) : void 0;
        if (u) return u;
        if (a.validators.length === 0) return;
        const c = l.join(jc);
        return (_a = a.validators.find(({ validator: d }) => d(c))) == null ? void 0 : _a.classGroupId;
      }, Jg = /^\[(.+)\]$/, e0 = (l) => {
        if (Jg.test(l)) {
          const a = Jg.exec(l)[1], o = a == null ? void 0 : a.substring(0, a.indexOf(":"));
          if (o) return "arbitrary.." + o;
        }
      }, t0 = (l) => {
        const { theme: a, classGroups: o } = l, i = {
          nextPart: /* @__PURE__ */ new Map(),
          validators: []
        };
        for (const u in o) Sc(o[u], i, u, a);
        return i;
      }, Sc = (l, a, o, i) => {
        l.forEach((u) => {
          if (typeof u == "string") {
            const c = u === "" ? a : em(a, u);
            c.classGroupId = o;
            return;
          }
          if (typeof u == "function") {
            if (n0(u)) {
              Sc(u(i), a, o, i);
              return;
            }
            a.validators.push({
              validator: u,
              classGroupId: o
            });
            return;
          }
          Object.entries(u).forEach(([c, d]) => {
            Sc(d, em(a, c), o, i);
          });
        });
      }, em = (l, a) => {
        let o = l;
        return a.split(jc).forEach((i) => {
          o.nextPart.has(i) || o.nextPart.set(i, {
            nextPart: /* @__PURE__ */ new Map(),
            validators: []
          }), o = o.nextPart.get(i);
        }), o;
      }, n0 = (l) => l.isThemeGetter, i0 = (l) => {
        if (l < 1) return {
          get: () => {
          },
          set: () => {
          }
        };
        let a = 0, o = /* @__PURE__ */ new Map(), i = /* @__PURE__ */ new Map();
        const u = (c, d) => {
          o.set(c, d), a++, a > l && (a = 0, i = o, o = /* @__PURE__ */ new Map());
        };
        return {
          get(c) {
            let d = o.get(c);
            if (d !== void 0) return d;
            if ((d = i.get(c)) !== void 0) return u(c, d), d;
          },
          set(c, d) {
            o.has(c) ? o.set(c, d) : u(c, d);
          }
        };
      }, xc = "!", Tc = ":", a0 = Tc.length, r0 = (l) => {
        const { prefix: a, experimentalParseClassName: o } = l;
        let i = (u) => {
          const c = [];
          let d = 0, h = 0, m = 0, g;
          for (let z = 0; z < u.length; z++) {
            let H = u[z];
            if (d === 0 && h === 0) {
              if (H === Tc) {
                c.push(u.slice(m, z)), m = z + a0;
                continue;
              }
              if (H === "/") {
                g = z;
                continue;
              }
            }
            H === "[" ? d++ : H === "]" ? d-- : H === "(" ? h++ : H === ")" && h--;
          }
          const p = c.length === 0 ? u : u.substring(m), _ = l0(p), E = _ !== p, R = g && g > m ? g - m : void 0;
          return {
            modifiers: c,
            hasImportantModifier: E,
            baseClassName: _,
            maybePostfixModifierPosition: R
          };
        };
        if (a) {
          const u = a + Tc, c = i;
          i = (d) => d.startsWith(u) ? c(d.substring(u.length)) : {
            isExternal: true,
            modifiers: [],
            hasImportantModifier: false,
            baseClassName: d,
            maybePostfixModifierPosition: void 0
          };
        }
        if (o) {
          const u = i;
          i = (c) => o({
            className: c,
            parseClassName: u
          });
        }
        return i;
      }, l0 = (l) => l.endsWith(xc) ? l.substring(0, l.length - 1) : l.startsWith(xc) ? l.substring(1) : l, o0 = (l) => {
        const a = Object.fromEntries(l.orderSensitiveModifiers.map((i) => [
          i,
          true
        ]));
        return (i) => {
          if (i.length <= 1) return i;
          const u = [];
          let c = [];
          return i.forEach((d) => {
            d[0] === "[" || a[d] ? (u.push(...c.sort(), d), c = []) : c.push(d);
          }), u.push(...c.sort()), u;
        };
      }, u0 = (l) => ({
        cache: i0(l.cacheSize),
        parseClassName: r0(l),
        sortModifiers: o0(l),
        ...J_(l)
      }), s0 = /\s+/, c0 = (l, a) => {
        const { parseClassName: o, getClassGroupId: i, getConflictingClassGroupIds: u, sortModifiers: c } = a, d = [], h = l.trim().split(s0);
        let m = "";
        for (let g = h.length - 1; g >= 0; g -= 1) {
          const p = h[g], { isExternal: _, modifiers: E, hasImportantModifier: R, baseClassName: z, maybePostfixModifierPosition: H } = o(p);
          if (_) {
            m = p + (m.length > 0 ? " " + m : m);
            continue;
          }
          let Y = !!H, te = i(Y ? z.substring(0, H) : z);
          if (!te) {
            if (!Y) {
              m = p + (m.length > 0 ? " " + m : m);
              continue;
            }
            if (te = i(z), !te) {
              m = p + (m.length > 0 ? " " + m : m);
              continue;
            }
            Y = false;
          }
          const J = c(E).join(":"), I = R ? J + xc : J, G = I + te;
          if (d.includes(G)) continue;
          d.push(G);
          const B = u(te, Y);
          for (let Z = 0; Z < B.length; ++Z) {
            const T = B[Z];
            d.push(I + T);
          }
          m = p + (m.length > 0 ? " " + m : m);
        }
        return m;
      };
      function f0() {
        let l = 0, a, o, i = "";
        for (; l < arguments.length; ) (a = arguments[l++]) && (o = tv(a)) && (i && (i += " "), i += o);
        return i;
      }
      const tv = (l) => {
        if (typeof l == "string") return l;
        let a, o = "";
        for (let i = 0; i < l.length; i++) l[i] && (a = tv(l[i])) && (o && (o += " "), o += a);
        return o;
      };
      function d0(l, ...a) {
        let o, i, u, c = d;
        function d(m) {
          const g = a.reduce((p, _) => _(p), l());
          return o = u0(g), i = o.cache.get, u = o.cache.set, c = h, h(m);
        }
        function h(m) {
          const g = i(m);
          if (g) return g;
          const p = c0(m, o);
          return u(m, p), p;
        }
        return function() {
          return c(f0.apply(null, arguments));
        };
      }
      const dt = (l) => {
        const a = (o) => o[l] || [];
        return a.isThemeGetter = true, a;
      }, nv = /^\[(?:(\w[\w-]*):)?(.+)\]$/i, iv = /^\((?:(\w[\w-]*):)?(.+)\)$/i, h0 = /^\d+\/\d+$/, g0 = /^(\d+(\.\d+)?)?(xs|sm|md|lg|xl)$/, m0 = /\d+(%|px|r?em|[sdl]?v([hwib]|min|max)|pt|pc|in|cm|mm|cap|ch|ex|r?lh|cq(w|h|i|b|min|max))|\b(calc|min|max|clamp)\(.+\)|^0$/, v0 = /^(rgba?|hsla?|hwb|(ok)?(lab|lch))\(.+\)$/, p0 = /^(inset_)?-?((\d+)?\.?(\d+)[a-z]+|0)_-?((\d+)?\.?(\d+)[a-z]+|0)/, y0 = /^(url|image|image-set|cross-fade|element|(repeating-)?(linear|radial|conic)-gradient)\(.+\)$/, Ra = (l) => h0.test(l), De = (l) => !!l && !Number.isNaN(Number(l)), pi = (l) => !!l && Number.isInteger(Number(l)), Ws = (l) => l.endsWith("%") && De(l.slice(0, -1)), Fn = (l) => g0.test(l), b0 = () => true, _0 = (l) => m0.test(l) && !v0.test(l), av = () => false, w0 = (l) => p0.test(l), E0 = (l) => y0.test(l), S0 = (l) => !ge(l) && !me(l), x0 = (l) => Ma(l, ov, av), ge = (l) => nv.test(l), ji = (l) => Ma(l, uv, _0), Js = (l) => Ma(l, D0, De), tm = (l) => Ma(l, rv, av), T0 = (l) => Ma(l, lv, E0), co = (l) => Ma(l, sv, w0), me = (l) => iv.test(l), Ur = (l) => La(l, uv), A0 = (l) => La(l, N0), nm = (l) => La(l, rv), R0 = (l) => La(l, ov), C0 = (l) => La(l, lv), fo = (l) => La(l, sv, true), Ma = (l, a, o) => {
        const i = nv.exec(l);
        return i ? i[1] ? a(i[1]) : o(i[2]) : false;
      }, La = (l, a, o = false) => {
        const i = iv.exec(l);
        return i ? i[1] ? a(i[1]) : o : false;
      }, rv = (l) => l === "position" || l === "percentage", lv = (l) => l === "image" || l === "url", ov = (l) => l === "length" || l === "size" || l === "bg-size", uv = (l) => l === "length", D0 = (l) => l === "number", N0 = (l) => l === "family-name", sv = (l) => l === "shadow", O0 = () => {
        const l = dt("color"), a = dt("font"), o = dt("text"), i = dt("font-weight"), u = dt("tracking"), c = dt("leading"), d = dt("breakpoint"), h = dt("container"), m = dt("spacing"), g = dt("radius"), p = dt("shadow"), _ = dt("inset-shadow"), E = dt("text-shadow"), R = dt("drop-shadow"), z = dt("blur"), H = dt("perspective"), Y = dt("aspect"), te = dt("ease"), J = dt("animate"), I = () => [
          "auto",
          "avoid",
          "all",
          "avoid-page",
          "page",
          "left",
          "right",
          "column"
        ], G = () => [
          "center",
          "top",
          "bottom",
          "left",
          "right",
          "top-left",
          "left-top",
          "top-right",
          "right-top",
          "bottom-right",
          "right-bottom",
          "bottom-left",
          "left-bottom"
        ], B = () => [
          ...G(),
          me,
          ge
        ], Z = () => [
          "auto",
          "hidden",
          "clip",
          "visible",
          "scroll"
        ], T = () => [
          "auto",
          "contain",
          "none"
        ], y = () => [
          me,
          ge,
          m
        ], w = () => [
          Ra,
          "full",
          "auto",
          ...y()
        ], S = () => [
          pi,
          "none",
          "subgrid",
          me,
          ge
        ], N = () => [
          "auto",
          {
            span: [
              "full",
              pi,
              me,
              ge
            ]
          },
          pi,
          me,
          ge
        ], L = () => [
          pi,
          "auto",
          me,
          ge
        ], re = () => [
          "auto",
          "min",
          "max",
          "fr",
          me,
          ge
        ], ue = () => [
          "start",
          "end",
          "center",
          "between",
          "around",
          "evenly",
          "stretch",
          "baseline",
          "center-safe",
          "end-safe"
        ], oe = () => [
          "start",
          "end",
          "center",
          "stretch",
          "center-safe",
          "end-safe"
        ], D = () => [
          "auto",
          ...y()
        ], X = () => [
          Ra,
          "auto",
          "full",
          "dvw",
          "dvh",
          "lvw",
          "lvh",
          "svw",
          "svh",
          "min",
          "max",
          "fit",
          ...y()
        ], $ = () => [
          l,
          me,
          ge
        ], ne = () => [
          ...G(),
          nm,
          tm,
          {
            position: [
              me,
              ge
            ]
          }
        ], x = () => [
          "no-repeat",
          {
            repeat: [
              "",
              "x",
              "y",
              "space",
              "round"
            ]
          }
        ], k = () => [
          "auto",
          "cover",
          "contain",
          R0,
          x0,
          {
            size: [
              me,
              ge
            ]
          }
        ], K = () => [
          Ws,
          Ur,
          ji
        ], U = () => [
          "",
          "none",
          "full",
          g,
          me,
          ge
        ], ie = () => [
          "",
          De,
          Ur,
          ji
        ], de = () => [
          "solid",
          "dashed",
          "dotted",
          "double"
        ], ce = () => [
          "normal",
          "multiply",
          "screen",
          "overlay",
          "darken",
          "lighten",
          "color-dodge",
          "color-burn",
          "hard-light",
          "soft-light",
          "difference",
          "exclusion",
          "hue",
          "saturation",
          "color",
          "luminosity"
        ], be = () => [
          De,
          Ws,
          nm,
          tm
        ], fe = () => [
          "",
          "none",
          z,
          me,
          ge
        ], _e = () => [
          "none",
          De,
          me,
          ge
        ], ke = () => [
          "none",
          De,
          me,
          ge
        ], pe = () => [
          De,
          me,
          ge
        ], Me = () => [
          Ra,
          "full",
          ...y()
        ];
        return {
          cacheSize: 500,
          theme: {
            animate: [
              "spin",
              "ping",
              "pulse",
              "bounce"
            ],
            aspect: [
              "video"
            ],
            blur: [
              Fn
            ],
            breakpoint: [
              Fn
            ],
            color: [
              b0
            ],
            container: [
              Fn
            ],
            "drop-shadow": [
              Fn
            ],
            ease: [
              "in",
              "out",
              "in-out"
            ],
            font: [
              S0
            ],
            "font-weight": [
              "thin",
              "extralight",
              "light",
              "normal",
              "medium",
              "semibold",
              "bold",
              "extrabold",
              "black"
            ],
            "inset-shadow": [
              Fn
            ],
            leading: [
              "none",
              "tight",
              "snug",
              "normal",
              "relaxed",
              "loose"
            ],
            perspective: [
              "dramatic",
              "near",
              "normal",
              "midrange",
              "distant",
              "none"
            ],
            radius: [
              Fn
            ],
            shadow: [
              Fn
            ],
            spacing: [
              "px",
              De
            ],
            text: [
              Fn
            ],
            "text-shadow": [
              Fn
            ],
            tracking: [
              "tighter",
              "tight",
              "normal",
              "wide",
              "wider",
              "widest"
            ]
          },
          classGroups: {
            aspect: [
              {
                aspect: [
                  "auto",
                  "square",
                  Ra,
                  ge,
                  me,
                  Y
                ]
              }
            ],
            container: [
              "container"
            ],
            columns: [
              {
                columns: [
                  De,
                  ge,
                  me,
                  h
                ]
              }
            ],
            "break-after": [
              {
                "break-after": I()
              }
            ],
            "break-before": [
              {
                "break-before": I()
              }
            ],
            "break-inside": [
              {
                "break-inside": [
                  "auto",
                  "avoid",
                  "avoid-page",
                  "avoid-column"
                ]
              }
            ],
            "box-decoration": [
              {
                "box-decoration": [
                  "slice",
                  "clone"
                ]
              }
            ],
            box: [
              {
                box: [
                  "border",
                  "content"
                ]
              }
            ],
            display: [
              "block",
              "inline-block",
              "inline",
              "flex",
              "inline-flex",
              "table",
              "inline-table",
              "table-caption",
              "table-cell",
              "table-column",
              "table-column-group",
              "table-footer-group",
              "table-header-group",
              "table-row-group",
              "table-row",
              "flow-root",
              "grid",
              "inline-grid",
              "contents",
              "list-item",
              "hidden"
            ],
            sr: [
              "sr-only",
              "not-sr-only"
            ],
            float: [
              {
                float: [
                  "right",
                  "left",
                  "none",
                  "start",
                  "end"
                ]
              }
            ],
            clear: [
              {
                clear: [
                  "left",
                  "right",
                  "both",
                  "none",
                  "start",
                  "end"
                ]
              }
            ],
            isolation: [
              "isolate",
              "isolation-auto"
            ],
            "object-fit": [
              {
                object: [
                  "contain",
                  "cover",
                  "fill",
                  "none",
                  "scale-down"
                ]
              }
            ],
            "object-position": [
              {
                object: B()
              }
            ],
            overflow: [
              {
                overflow: Z()
              }
            ],
            "overflow-x": [
              {
                "overflow-x": Z()
              }
            ],
            "overflow-y": [
              {
                "overflow-y": Z()
              }
            ],
            overscroll: [
              {
                overscroll: T()
              }
            ],
            "overscroll-x": [
              {
                "overscroll-x": T()
              }
            ],
            "overscroll-y": [
              {
                "overscroll-y": T()
              }
            ],
            position: [
              "static",
              "fixed",
              "absolute",
              "relative",
              "sticky"
            ],
            inset: [
              {
                inset: w()
              }
            ],
            "inset-x": [
              {
                "inset-x": w()
              }
            ],
            "inset-y": [
              {
                "inset-y": w()
              }
            ],
            start: [
              {
                start: w()
              }
            ],
            end: [
              {
                end: w()
              }
            ],
            top: [
              {
                top: w()
              }
            ],
            right: [
              {
                right: w()
              }
            ],
            bottom: [
              {
                bottom: w()
              }
            ],
            left: [
              {
                left: w()
              }
            ],
            visibility: [
              "visible",
              "invisible",
              "collapse"
            ],
            z: [
              {
                z: [
                  pi,
                  "auto",
                  me,
                  ge
                ]
              }
            ],
            basis: [
              {
                basis: [
                  Ra,
                  "full",
                  "auto",
                  h,
                  ...y()
                ]
              }
            ],
            "flex-direction": [
              {
                flex: [
                  "row",
                  "row-reverse",
                  "col",
                  "col-reverse"
                ]
              }
            ],
            "flex-wrap": [
              {
                flex: [
                  "nowrap",
                  "wrap",
                  "wrap-reverse"
                ]
              }
            ],
            flex: [
              {
                flex: [
                  De,
                  Ra,
                  "auto",
                  "initial",
                  "none",
                  ge
                ]
              }
            ],
            grow: [
              {
                grow: [
                  "",
                  De,
                  me,
                  ge
                ]
              }
            ],
            shrink: [
              {
                shrink: [
                  "",
                  De,
                  me,
                  ge
                ]
              }
            ],
            order: [
              {
                order: [
                  pi,
                  "first",
                  "last",
                  "none",
                  me,
                  ge
                ]
              }
            ],
            "grid-cols": [
              {
                "grid-cols": S()
              }
            ],
            "col-start-end": [
              {
                col: N()
              }
            ],
            "col-start": [
              {
                "col-start": L()
              }
            ],
            "col-end": [
              {
                "col-end": L()
              }
            ],
            "grid-rows": [
              {
                "grid-rows": S()
              }
            ],
            "row-start-end": [
              {
                row: N()
              }
            ],
            "row-start": [
              {
                "row-start": L()
              }
            ],
            "row-end": [
              {
                "row-end": L()
              }
            ],
            "grid-flow": [
              {
                "grid-flow": [
                  "row",
                  "col",
                  "dense",
                  "row-dense",
                  "col-dense"
                ]
              }
            ],
            "auto-cols": [
              {
                "auto-cols": re()
              }
            ],
            "auto-rows": [
              {
                "auto-rows": re()
              }
            ],
            gap: [
              {
                gap: y()
              }
            ],
            "gap-x": [
              {
                "gap-x": y()
              }
            ],
            "gap-y": [
              {
                "gap-y": y()
              }
            ],
            "justify-content": [
              {
                justify: [
                  ...ue(),
                  "normal"
                ]
              }
            ],
            "justify-items": [
              {
                "justify-items": [
                  ...oe(),
                  "normal"
                ]
              }
            ],
            "justify-self": [
              {
                "justify-self": [
                  "auto",
                  ...oe()
                ]
              }
            ],
            "align-content": [
              {
                content: [
                  "normal",
                  ...ue()
                ]
              }
            ],
            "align-items": [
              {
                items: [
                  ...oe(),
                  {
                    baseline: [
                      "",
                      "last"
                    ]
                  }
                ]
              }
            ],
            "align-self": [
              {
                self: [
                  "auto",
                  ...oe(),
                  {
                    baseline: [
                      "",
                      "last"
                    ]
                  }
                ]
              }
            ],
            "place-content": [
              {
                "place-content": ue()
              }
            ],
            "place-items": [
              {
                "place-items": [
                  ...oe(),
                  "baseline"
                ]
              }
            ],
            "place-self": [
              {
                "place-self": [
                  "auto",
                  ...oe()
                ]
              }
            ],
            p: [
              {
                p: y()
              }
            ],
            px: [
              {
                px: y()
              }
            ],
            py: [
              {
                py: y()
              }
            ],
            ps: [
              {
                ps: y()
              }
            ],
            pe: [
              {
                pe: y()
              }
            ],
            pt: [
              {
                pt: y()
              }
            ],
            pr: [
              {
                pr: y()
              }
            ],
            pb: [
              {
                pb: y()
              }
            ],
            pl: [
              {
                pl: y()
              }
            ],
            m: [
              {
                m: D()
              }
            ],
            mx: [
              {
                mx: D()
              }
            ],
            my: [
              {
                my: D()
              }
            ],
            ms: [
              {
                ms: D()
              }
            ],
            me: [
              {
                me: D()
              }
            ],
            mt: [
              {
                mt: D()
              }
            ],
            mr: [
              {
                mr: D()
              }
            ],
            mb: [
              {
                mb: D()
              }
            ],
            ml: [
              {
                ml: D()
              }
            ],
            "space-x": [
              {
                "space-x": y()
              }
            ],
            "space-x-reverse": [
              "space-x-reverse"
            ],
            "space-y": [
              {
                "space-y": y()
              }
            ],
            "space-y-reverse": [
              "space-y-reverse"
            ],
            size: [
              {
                size: X()
              }
            ],
            w: [
              {
                w: [
                  h,
                  "screen",
                  ...X()
                ]
              }
            ],
            "min-w": [
              {
                "min-w": [
                  h,
                  "screen",
                  "none",
                  ...X()
                ]
              }
            ],
            "max-w": [
              {
                "max-w": [
                  h,
                  "screen",
                  "none",
                  "prose",
                  {
                    screen: [
                      d
                    ]
                  },
                  ...X()
                ]
              }
            ],
            h: [
              {
                h: [
                  "screen",
                  "lh",
                  ...X()
                ]
              }
            ],
            "min-h": [
              {
                "min-h": [
                  "screen",
                  "lh",
                  "none",
                  ...X()
                ]
              }
            ],
            "max-h": [
              {
                "max-h": [
                  "screen",
                  "lh",
                  ...X()
                ]
              }
            ],
            "font-size": [
              {
                text: [
                  "base",
                  o,
                  Ur,
                  ji
                ]
              }
            ],
            "font-smoothing": [
              "antialiased",
              "subpixel-antialiased"
            ],
            "font-style": [
              "italic",
              "not-italic"
            ],
            "font-weight": [
              {
                font: [
                  i,
                  me,
                  Js
                ]
              }
            ],
            "font-stretch": [
              {
                "font-stretch": [
                  "ultra-condensed",
                  "extra-condensed",
                  "condensed",
                  "semi-condensed",
                  "normal",
                  "semi-expanded",
                  "expanded",
                  "extra-expanded",
                  "ultra-expanded",
                  Ws,
                  ge
                ]
              }
            ],
            "font-family": [
              {
                font: [
                  A0,
                  ge,
                  a
                ]
              }
            ],
            "fvn-normal": [
              "normal-nums"
            ],
            "fvn-ordinal": [
              "ordinal"
            ],
            "fvn-slashed-zero": [
              "slashed-zero"
            ],
            "fvn-figure": [
              "lining-nums",
              "oldstyle-nums"
            ],
            "fvn-spacing": [
              "proportional-nums",
              "tabular-nums"
            ],
            "fvn-fraction": [
              "diagonal-fractions",
              "stacked-fractions"
            ],
            tracking: [
              {
                tracking: [
                  u,
                  me,
                  ge
                ]
              }
            ],
            "line-clamp": [
              {
                "line-clamp": [
                  De,
                  "none",
                  me,
                  Js
                ]
              }
            ],
            leading: [
              {
                leading: [
                  c,
                  ...y()
                ]
              }
            ],
            "list-image": [
              {
                "list-image": [
                  "none",
                  me,
                  ge
                ]
              }
            ],
            "list-style-position": [
              {
                list: [
                  "inside",
                  "outside"
                ]
              }
            ],
            "list-style-type": [
              {
                list: [
                  "disc",
                  "decimal",
                  "none",
                  me,
                  ge
                ]
              }
            ],
            "text-alignment": [
              {
                text: [
                  "left",
                  "center",
                  "right",
                  "justify",
                  "start",
                  "end"
                ]
              }
            ],
            "placeholder-color": [
              {
                placeholder: $()
              }
            ],
            "text-color": [
              {
                text: $()
              }
            ],
            "text-decoration": [
              "underline",
              "overline",
              "line-through",
              "no-underline"
            ],
            "text-decoration-style": [
              {
                decoration: [
                  ...de(),
                  "wavy"
                ]
              }
            ],
            "text-decoration-thickness": [
              {
                decoration: [
                  De,
                  "from-font",
                  "auto",
                  me,
                  ji
                ]
              }
            ],
            "text-decoration-color": [
              {
                decoration: $()
              }
            ],
            "underline-offset": [
              {
                "underline-offset": [
                  De,
                  "auto",
                  me,
                  ge
                ]
              }
            ],
            "text-transform": [
              "uppercase",
              "lowercase",
              "capitalize",
              "normal-case"
            ],
            "text-overflow": [
              "truncate",
              "text-ellipsis",
              "text-clip"
            ],
            "text-wrap": [
              {
                text: [
                  "wrap",
                  "nowrap",
                  "balance",
                  "pretty"
                ]
              }
            ],
            indent: [
              {
                indent: y()
              }
            ],
            "vertical-align": [
              {
                align: [
                  "baseline",
                  "top",
                  "middle",
                  "bottom",
                  "text-top",
                  "text-bottom",
                  "sub",
                  "super",
                  me,
                  ge
                ]
              }
            ],
            whitespace: [
              {
                whitespace: [
                  "normal",
                  "nowrap",
                  "pre",
                  "pre-line",
                  "pre-wrap",
                  "break-spaces"
                ]
              }
            ],
            break: [
              {
                break: [
                  "normal",
                  "words",
                  "all",
                  "keep"
                ]
              }
            ],
            wrap: [
              {
                wrap: [
                  "break-word",
                  "anywhere",
                  "normal"
                ]
              }
            ],
            hyphens: [
              {
                hyphens: [
                  "none",
                  "manual",
                  "auto"
                ]
              }
            ],
            content: [
              {
                content: [
                  "none",
                  me,
                  ge
                ]
              }
            ],
            "bg-attachment": [
              {
                bg: [
                  "fixed",
                  "local",
                  "scroll"
                ]
              }
            ],
            "bg-clip": [
              {
                "bg-clip": [
                  "border",
                  "padding",
                  "content",
                  "text"
                ]
              }
            ],
            "bg-origin": [
              {
                "bg-origin": [
                  "border",
                  "padding",
                  "content"
                ]
              }
            ],
            "bg-position": [
              {
                bg: ne()
              }
            ],
            "bg-repeat": [
              {
                bg: x()
              }
            ],
            "bg-size": [
              {
                bg: k()
              }
            ],
            "bg-image": [
              {
                bg: [
                  "none",
                  {
                    linear: [
                      {
                        to: [
                          "t",
                          "tr",
                          "r",
                          "br",
                          "b",
                          "bl",
                          "l",
                          "tl"
                        ]
                      },
                      pi,
                      me,
                      ge
                    ],
                    radial: [
                      "",
                      me,
                      ge
                    ],
                    conic: [
                      pi,
                      me,
                      ge
                    ]
                  },
                  C0,
                  T0
                ]
              }
            ],
            "bg-color": [
              {
                bg: $()
              }
            ],
            "gradient-from-pos": [
              {
                from: K()
              }
            ],
            "gradient-via-pos": [
              {
                via: K()
              }
            ],
            "gradient-to-pos": [
              {
                to: K()
              }
            ],
            "gradient-from": [
              {
                from: $()
              }
            ],
            "gradient-via": [
              {
                via: $()
              }
            ],
            "gradient-to": [
              {
                to: $()
              }
            ],
            rounded: [
              {
                rounded: U()
              }
            ],
            "rounded-s": [
              {
                "rounded-s": U()
              }
            ],
            "rounded-e": [
              {
                "rounded-e": U()
              }
            ],
            "rounded-t": [
              {
                "rounded-t": U()
              }
            ],
            "rounded-r": [
              {
                "rounded-r": U()
              }
            ],
            "rounded-b": [
              {
                "rounded-b": U()
              }
            ],
            "rounded-l": [
              {
                "rounded-l": U()
              }
            ],
            "rounded-ss": [
              {
                "rounded-ss": U()
              }
            ],
            "rounded-se": [
              {
                "rounded-se": U()
              }
            ],
            "rounded-ee": [
              {
                "rounded-ee": U()
              }
            ],
            "rounded-es": [
              {
                "rounded-es": U()
              }
            ],
            "rounded-tl": [
              {
                "rounded-tl": U()
              }
            ],
            "rounded-tr": [
              {
                "rounded-tr": U()
              }
            ],
            "rounded-br": [
              {
                "rounded-br": U()
              }
            ],
            "rounded-bl": [
              {
                "rounded-bl": U()
              }
            ],
            "border-w": [
              {
                border: ie()
              }
            ],
            "border-w-x": [
              {
                "border-x": ie()
              }
            ],
            "border-w-y": [
              {
                "border-y": ie()
              }
            ],
            "border-w-s": [
              {
                "border-s": ie()
              }
            ],
            "border-w-e": [
              {
                "border-e": ie()
              }
            ],
            "border-w-t": [
              {
                "border-t": ie()
              }
            ],
            "border-w-r": [
              {
                "border-r": ie()
              }
            ],
            "border-w-b": [
              {
                "border-b": ie()
              }
            ],
            "border-w-l": [
              {
                "border-l": ie()
              }
            ],
            "divide-x": [
              {
                "divide-x": ie()
              }
            ],
            "divide-x-reverse": [
              "divide-x-reverse"
            ],
            "divide-y": [
              {
                "divide-y": ie()
              }
            ],
            "divide-y-reverse": [
              "divide-y-reverse"
            ],
            "border-style": [
              {
                border: [
                  ...de(),
                  "hidden",
                  "none"
                ]
              }
            ],
            "divide-style": [
              {
                divide: [
                  ...de(),
                  "hidden",
                  "none"
                ]
              }
            ],
            "border-color": [
              {
                border: $()
              }
            ],
            "border-color-x": [
              {
                "border-x": $()
              }
            ],
            "border-color-y": [
              {
                "border-y": $()
              }
            ],
            "border-color-s": [
              {
                "border-s": $()
              }
            ],
            "border-color-e": [
              {
                "border-e": $()
              }
            ],
            "border-color-t": [
              {
                "border-t": $()
              }
            ],
            "border-color-r": [
              {
                "border-r": $()
              }
            ],
            "border-color-b": [
              {
                "border-b": $()
              }
            ],
            "border-color-l": [
              {
                "border-l": $()
              }
            ],
            "divide-color": [
              {
                divide: $()
              }
            ],
            "outline-style": [
              {
                outline: [
                  ...de(),
                  "none",
                  "hidden"
                ]
              }
            ],
            "outline-offset": [
              {
                "outline-offset": [
                  De,
                  me,
                  ge
                ]
              }
            ],
            "outline-w": [
              {
                outline: [
                  "",
                  De,
                  Ur,
                  ji
                ]
              }
            ],
            "outline-color": [
              {
                outline: $()
              }
            ],
            shadow: [
              {
                shadow: [
                  "",
                  "none",
                  p,
                  fo,
                  co
                ]
              }
            ],
            "shadow-color": [
              {
                shadow: $()
              }
            ],
            "inset-shadow": [
              {
                "inset-shadow": [
                  "none",
                  _,
                  fo,
                  co
                ]
              }
            ],
            "inset-shadow-color": [
              {
                "inset-shadow": $()
              }
            ],
            "ring-w": [
              {
                ring: ie()
              }
            ],
            "ring-w-inset": [
              "ring-inset"
            ],
            "ring-color": [
              {
                ring: $()
              }
            ],
            "ring-offset-w": [
              {
                "ring-offset": [
                  De,
                  ji
                ]
              }
            ],
            "ring-offset-color": [
              {
                "ring-offset": $()
              }
            ],
            "inset-ring-w": [
              {
                "inset-ring": ie()
              }
            ],
            "inset-ring-color": [
              {
                "inset-ring": $()
              }
            ],
            "text-shadow": [
              {
                "text-shadow": [
                  "none",
                  E,
                  fo,
                  co
                ]
              }
            ],
            "text-shadow-color": [
              {
                "text-shadow": $()
              }
            ],
            opacity: [
              {
                opacity: [
                  De,
                  me,
                  ge
                ]
              }
            ],
            "mix-blend": [
              {
                "mix-blend": [
                  ...ce(),
                  "plus-darker",
                  "plus-lighter"
                ]
              }
            ],
            "bg-blend": [
              {
                "bg-blend": ce()
              }
            ],
            "mask-clip": [
              {
                "mask-clip": [
                  "border",
                  "padding",
                  "content",
                  "fill",
                  "stroke",
                  "view"
                ]
              },
              "mask-no-clip"
            ],
            "mask-composite": [
              {
                mask: [
                  "add",
                  "subtract",
                  "intersect",
                  "exclude"
                ]
              }
            ],
            "mask-image-linear-pos": [
              {
                "mask-linear": [
                  De
                ]
              }
            ],
            "mask-image-linear-from-pos": [
              {
                "mask-linear-from": be()
              }
            ],
            "mask-image-linear-to-pos": [
              {
                "mask-linear-to": be()
              }
            ],
            "mask-image-linear-from-color": [
              {
                "mask-linear-from": $()
              }
            ],
            "mask-image-linear-to-color": [
              {
                "mask-linear-to": $()
              }
            ],
            "mask-image-t-from-pos": [
              {
                "mask-t-from": be()
              }
            ],
            "mask-image-t-to-pos": [
              {
                "mask-t-to": be()
              }
            ],
            "mask-image-t-from-color": [
              {
                "mask-t-from": $()
              }
            ],
            "mask-image-t-to-color": [
              {
                "mask-t-to": $()
              }
            ],
            "mask-image-r-from-pos": [
              {
                "mask-r-from": be()
              }
            ],
            "mask-image-r-to-pos": [
              {
                "mask-r-to": be()
              }
            ],
            "mask-image-r-from-color": [
              {
                "mask-r-from": $()
              }
            ],
            "mask-image-r-to-color": [
              {
                "mask-r-to": $()
              }
            ],
            "mask-image-b-from-pos": [
              {
                "mask-b-from": be()
              }
            ],
            "mask-image-b-to-pos": [
              {
                "mask-b-to": be()
              }
            ],
            "mask-image-b-from-color": [
              {
                "mask-b-from": $()
              }
            ],
            "mask-image-b-to-color": [
              {
                "mask-b-to": $()
              }
            ],
            "mask-image-l-from-pos": [
              {
                "mask-l-from": be()
              }
            ],
            "mask-image-l-to-pos": [
              {
                "mask-l-to": be()
              }
            ],
            "mask-image-l-from-color": [
              {
                "mask-l-from": $()
              }
            ],
            "mask-image-l-to-color": [
              {
                "mask-l-to": $()
              }
            ],
            "mask-image-x-from-pos": [
              {
                "mask-x-from": be()
              }
            ],
            "mask-image-x-to-pos": [
              {
                "mask-x-to": be()
              }
            ],
            "mask-image-x-from-color": [
              {
                "mask-x-from": $()
              }
            ],
            "mask-image-x-to-color": [
              {
                "mask-x-to": $()
              }
            ],
            "mask-image-y-from-pos": [
              {
                "mask-y-from": be()
              }
            ],
            "mask-image-y-to-pos": [
              {
                "mask-y-to": be()
              }
            ],
            "mask-image-y-from-color": [
              {
                "mask-y-from": $()
              }
            ],
            "mask-image-y-to-color": [
              {
                "mask-y-to": $()
              }
            ],
            "mask-image-radial": [
              {
                "mask-radial": [
                  me,
                  ge
                ]
              }
            ],
            "mask-image-radial-from-pos": [
              {
                "mask-radial-from": be()
              }
            ],
            "mask-image-radial-to-pos": [
              {
                "mask-radial-to": be()
              }
            ],
            "mask-image-radial-from-color": [
              {
                "mask-radial-from": $()
              }
            ],
            "mask-image-radial-to-color": [
              {
                "mask-radial-to": $()
              }
            ],
            "mask-image-radial-shape": [
              {
                "mask-radial": [
                  "circle",
                  "ellipse"
                ]
              }
            ],
            "mask-image-radial-size": [
              {
                "mask-radial": [
                  {
                    closest: [
                      "side",
                      "corner"
                    ],
                    farthest: [
                      "side",
                      "corner"
                    ]
                  }
                ]
              }
            ],
            "mask-image-radial-pos": [
              {
                "mask-radial-at": G()
              }
            ],
            "mask-image-conic-pos": [
              {
                "mask-conic": [
                  De
                ]
              }
            ],
            "mask-image-conic-from-pos": [
              {
                "mask-conic-from": be()
              }
            ],
            "mask-image-conic-to-pos": [
              {
                "mask-conic-to": be()
              }
            ],
            "mask-image-conic-from-color": [
              {
                "mask-conic-from": $()
              }
            ],
            "mask-image-conic-to-color": [
              {
                "mask-conic-to": $()
              }
            ],
            "mask-mode": [
              {
                mask: [
                  "alpha",
                  "luminance",
                  "match"
                ]
              }
            ],
            "mask-origin": [
              {
                "mask-origin": [
                  "border",
                  "padding",
                  "content",
                  "fill",
                  "stroke",
                  "view"
                ]
              }
            ],
            "mask-position": [
              {
                mask: ne()
              }
            ],
            "mask-repeat": [
              {
                mask: x()
              }
            ],
            "mask-size": [
              {
                mask: k()
              }
            ],
            "mask-type": [
              {
                "mask-type": [
                  "alpha",
                  "luminance"
                ]
              }
            ],
            "mask-image": [
              {
                mask: [
                  "none",
                  me,
                  ge
                ]
              }
            ],
            filter: [
              {
                filter: [
                  "",
                  "none",
                  me,
                  ge
                ]
              }
            ],
            blur: [
              {
                blur: fe()
              }
            ],
            brightness: [
              {
                brightness: [
                  De,
                  me,
                  ge
                ]
              }
            ],
            contrast: [
              {
                contrast: [
                  De,
                  me,
                  ge
                ]
              }
            ],
            "drop-shadow": [
              {
                "drop-shadow": [
                  "",
                  "none",
                  R,
                  fo,
                  co
                ]
              }
            ],
            "drop-shadow-color": [
              {
                "drop-shadow": $()
              }
            ],
            grayscale: [
              {
                grayscale: [
                  "",
                  De,
                  me,
                  ge
                ]
              }
            ],
            "hue-rotate": [
              {
                "hue-rotate": [
                  De,
                  me,
                  ge
                ]
              }
            ],
            invert: [
              {
                invert: [
                  "",
                  De,
                  me,
                  ge
                ]
              }
            ],
            saturate: [
              {
                saturate: [
                  De,
                  me,
                  ge
                ]
              }
            ],
            sepia: [
              {
                sepia: [
                  "",
                  De,
                  me,
                  ge
                ]
              }
            ],
            "backdrop-filter": [
              {
                "backdrop-filter": [
                  "",
                  "none",
                  me,
                  ge
                ]
              }
            ],
            "backdrop-blur": [
              {
                "backdrop-blur": fe()
              }
            ],
            "backdrop-brightness": [
              {
                "backdrop-brightness": [
                  De,
                  me,
                  ge
                ]
              }
            ],
            "backdrop-contrast": [
              {
                "backdrop-contrast": [
                  De,
                  me,
                  ge
                ]
              }
            ],
            "backdrop-grayscale": [
              {
                "backdrop-grayscale": [
                  "",
                  De,
                  me,
                  ge
                ]
              }
            ],
            "backdrop-hue-rotate": [
              {
                "backdrop-hue-rotate": [
                  De,
                  me,
                  ge
                ]
              }
            ],
            "backdrop-invert": [
              {
                "backdrop-invert": [
                  "",
                  De,
                  me,
                  ge
                ]
              }
            ],
            "backdrop-opacity": [
              {
                "backdrop-opacity": [
                  De,
                  me,
                  ge
                ]
              }
            ],
            "backdrop-saturate": [
              {
                "backdrop-saturate": [
                  De,
                  me,
                  ge
                ]
              }
            ],
            "backdrop-sepia": [
              {
                "backdrop-sepia": [
                  "",
                  De,
                  me,
                  ge
                ]
              }
            ],
            "border-collapse": [
              {
                border: [
                  "collapse",
                  "separate"
                ]
              }
            ],
            "border-spacing": [
              {
                "border-spacing": y()
              }
            ],
            "border-spacing-x": [
              {
                "border-spacing-x": y()
              }
            ],
            "border-spacing-y": [
              {
                "border-spacing-y": y()
              }
            ],
            "table-layout": [
              {
                table: [
                  "auto",
                  "fixed"
                ]
              }
            ],
            caption: [
              {
                caption: [
                  "top",
                  "bottom"
                ]
              }
            ],
            transition: [
              {
                transition: [
                  "",
                  "all",
                  "colors",
                  "opacity",
                  "shadow",
                  "transform",
                  "none",
                  me,
                  ge
                ]
              }
            ],
            "transition-behavior": [
              {
                transition: [
                  "normal",
                  "discrete"
                ]
              }
            ],
            duration: [
              {
                duration: [
                  De,
                  "initial",
                  me,
                  ge
                ]
              }
            ],
            ease: [
              {
                ease: [
                  "linear",
                  "initial",
                  te,
                  me,
                  ge
                ]
              }
            ],
            delay: [
              {
                delay: [
                  De,
                  me,
                  ge
                ]
              }
            ],
            animate: [
              {
                animate: [
                  "none",
                  J,
                  me,
                  ge
                ]
              }
            ],
            backface: [
              {
                backface: [
                  "hidden",
                  "visible"
                ]
              }
            ],
            perspective: [
              {
                perspective: [
                  H,
                  me,
                  ge
                ]
              }
            ],
            "perspective-origin": [
              {
                "perspective-origin": B()
              }
            ],
            rotate: [
              {
                rotate: _e()
              }
            ],
            "rotate-x": [
              {
                "rotate-x": _e()
              }
            ],
            "rotate-y": [
              {
                "rotate-y": _e()
              }
            ],
            "rotate-z": [
              {
                "rotate-z": _e()
              }
            ],
            scale: [
              {
                scale: ke()
              }
            ],
            "scale-x": [
              {
                "scale-x": ke()
              }
            ],
            "scale-y": [
              {
                "scale-y": ke()
              }
            ],
            "scale-z": [
              {
                "scale-z": ke()
              }
            ],
            "scale-3d": [
              "scale-3d"
            ],
            skew: [
              {
                skew: pe()
              }
            ],
            "skew-x": [
              {
                "skew-x": pe()
              }
            ],
            "skew-y": [
              {
                "skew-y": pe()
              }
            ],
            transform: [
              {
                transform: [
                  me,
                  ge,
                  "",
                  "none",
                  "gpu",
                  "cpu"
                ]
              }
            ],
            "transform-origin": [
              {
                origin: B()
              }
            ],
            "transform-style": [
              {
                transform: [
                  "3d",
                  "flat"
                ]
              }
            ],
            translate: [
              {
                translate: Me()
              }
            ],
            "translate-x": [
              {
                "translate-x": Me()
              }
            ],
            "translate-y": [
              {
                "translate-y": Me()
              }
            ],
            "translate-z": [
              {
                "translate-z": Me()
              }
            ],
            "translate-none": [
              "translate-none"
            ],
            accent: [
              {
                accent: $()
              }
            ],
            appearance: [
              {
                appearance: [
                  "none",
                  "auto"
                ]
              }
            ],
            "caret-color": [
              {
                caret: $()
              }
            ],
            "color-scheme": [
              {
                scheme: [
                  "normal",
                  "dark",
                  "light",
                  "light-dark",
                  "only-dark",
                  "only-light"
                ]
              }
            ],
            cursor: [
              {
                cursor: [
                  "auto",
                  "default",
                  "pointer",
                  "wait",
                  "text",
                  "move",
                  "help",
                  "not-allowed",
                  "none",
                  "context-menu",
                  "progress",
                  "cell",
                  "crosshair",
                  "vertical-text",
                  "alias",
                  "copy",
                  "no-drop",
                  "grab",
                  "grabbing",
                  "all-scroll",
                  "col-resize",
                  "row-resize",
                  "n-resize",
                  "e-resize",
                  "s-resize",
                  "w-resize",
                  "ne-resize",
                  "nw-resize",
                  "se-resize",
                  "sw-resize",
                  "ew-resize",
                  "ns-resize",
                  "nesw-resize",
                  "nwse-resize",
                  "zoom-in",
                  "zoom-out",
                  me,
                  ge
                ]
              }
            ],
            "field-sizing": [
              {
                "field-sizing": [
                  "fixed",
                  "content"
                ]
              }
            ],
            "pointer-events": [
              {
                "pointer-events": [
                  "auto",
                  "none"
                ]
              }
            ],
            resize: [
              {
                resize: [
                  "none",
                  "",
                  "y",
                  "x"
                ]
              }
            ],
            "scroll-behavior": [
              {
                scroll: [
                  "auto",
                  "smooth"
                ]
              }
            ],
            "scroll-m": [
              {
                "scroll-m": y()
              }
            ],
            "scroll-mx": [
              {
                "scroll-mx": y()
              }
            ],
            "scroll-my": [
              {
                "scroll-my": y()
              }
            ],
            "scroll-ms": [
              {
                "scroll-ms": y()
              }
            ],
            "scroll-me": [
              {
                "scroll-me": y()
              }
            ],
            "scroll-mt": [
              {
                "scroll-mt": y()
              }
            ],
            "scroll-mr": [
              {
                "scroll-mr": y()
              }
            ],
            "scroll-mb": [
              {
                "scroll-mb": y()
              }
            ],
            "scroll-ml": [
              {
                "scroll-ml": y()
              }
            ],
            "scroll-p": [
              {
                "scroll-p": y()
              }
            ],
            "scroll-px": [
              {
                "scroll-px": y()
              }
            ],
            "scroll-py": [
              {
                "scroll-py": y()
              }
            ],
            "scroll-ps": [
              {
                "scroll-ps": y()
              }
            ],
            "scroll-pe": [
              {
                "scroll-pe": y()
              }
            ],
            "scroll-pt": [
              {
                "scroll-pt": y()
              }
            ],
            "scroll-pr": [
              {
                "scroll-pr": y()
              }
            ],
            "scroll-pb": [
              {
                "scroll-pb": y()
              }
            ],
            "scroll-pl": [
              {
                "scroll-pl": y()
              }
            ],
            "snap-align": [
              {
                snap: [
                  "start",
                  "end",
                  "center",
                  "align-none"
                ]
              }
            ],
            "snap-stop": [
              {
                snap: [
                  "normal",
                  "always"
                ]
              }
            ],
            "snap-type": [
              {
                snap: [
                  "none",
                  "x",
                  "y",
                  "both"
                ]
              }
            ],
            "snap-strictness": [
              {
                snap: [
                  "mandatory",
                  "proximity"
                ]
              }
            ],
            touch: [
              {
                touch: [
                  "auto",
                  "none",
                  "manipulation"
                ]
              }
            ],
            "touch-x": [
              {
                "touch-pan": [
                  "x",
                  "left",
                  "right"
                ]
              }
            ],
            "touch-y": [
              {
                "touch-pan": [
                  "y",
                  "up",
                  "down"
                ]
              }
            ],
            "touch-pz": [
              "touch-pinch-zoom"
            ],
            select: [
              {
                select: [
                  "none",
                  "text",
                  "all",
                  "auto"
                ]
              }
            ],
            "will-change": [
              {
                "will-change": [
                  "auto",
                  "scroll",
                  "contents",
                  "transform",
                  me,
                  ge
                ]
              }
            ],
            fill: [
              {
                fill: [
                  "none",
                  ...$()
                ]
              }
            ],
            "stroke-w": [
              {
                stroke: [
                  De,
                  Ur,
                  ji,
                  Js
                ]
              }
            ],
            stroke: [
              {
                stroke: [
                  "none",
                  ...$()
                ]
              }
            ],
            "forced-color-adjust": [
              {
                "forced-color-adjust": [
                  "auto",
                  "none"
                ]
              }
            ]
          },
          conflictingClassGroups: {
            overflow: [
              "overflow-x",
              "overflow-y"
            ],
            overscroll: [
              "overscroll-x",
              "overscroll-y"
            ],
            inset: [
              "inset-x",
              "inset-y",
              "start",
              "end",
              "top",
              "right",
              "bottom",
              "left"
            ],
            "inset-x": [
              "right",
              "left"
            ],
            "inset-y": [
              "top",
              "bottom"
            ],
            flex: [
              "basis",
              "grow",
              "shrink"
            ],
            gap: [
              "gap-x",
              "gap-y"
            ],
            p: [
              "px",
              "py",
              "ps",
              "pe",
              "pt",
              "pr",
              "pb",
              "pl"
            ],
            px: [
              "pr",
              "pl"
            ],
            py: [
              "pt",
              "pb"
            ],
            m: [
              "mx",
              "my",
              "ms",
              "me",
              "mt",
              "mr",
              "mb",
              "ml"
            ],
            mx: [
              "mr",
              "ml"
            ],
            my: [
              "mt",
              "mb"
            ],
            size: [
              "w",
              "h"
            ],
            "font-size": [
              "leading"
            ],
            "fvn-normal": [
              "fvn-ordinal",
              "fvn-slashed-zero",
              "fvn-figure",
              "fvn-spacing",
              "fvn-fraction"
            ],
            "fvn-ordinal": [
              "fvn-normal"
            ],
            "fvn-slashed-zero": [
              "fvn-normal"
            ],
            "fvn-figure": [
              "fvn-normal"
            ],
            "fvn-spacing": [
              "fvn-normal"
            ],
            "fvn-fraction": [
              "fvn-normal"
            ],
            "line-clamp": [
              "display",
              "overflow"
            ],
            rounded: [
              "rounded-s",
              "rounded-e",
              "rounded-t",
              "rounded-r",
              "rounded-b",
              "rounded-l",
              "rounded-ss",
              "rounded-se",
              "rounded-ee",
              "rounded-es",
              "rounded-tl",
              "rounded-tr",
              "rounded-br",
              "rounded-bl"
            ],
            "rounded-s": [
              "rounded-ss",
              "rounded-es"
            ],
            "rounded-e": [
              "rounded-se",
              "rounded-ee"
            ],
            "rounded-t": [
              "rounded-tl",
              "rounded-tr"
            ],
            "rounded-r": [
              "rounded-tr",
              "rounded-br"
            ],
            "rounded-b": [
              "rounded-br",
              "rounded-bl"
            ],
            "rounded-l": [
              "rounded-tl",
              "rounded-bl"
            ],
            "border-spacing": [
              "border-spacing-x",
              "border-spacing-y"
            ],
            "border-w": [
              "border-w-x",
              "border-w-y",
              "border-w-s",
              "border-w-e",
              "border-w-t",
              "border-w-r",
              "border-w-b",
              "border-w-l"
            ],
            "border-w-x": [
              "border-w-r",
              "border-w-l"
            ],
            "border-w-y": [
              "border-w-t",
              "border-w-b"
            ],
            "border-color": [
              "border-color-x",
              "border-color-y",
              "border-color-s",
              "border-color-e",
              "border-color-t",
              "border-color-r",
              "border-color-b",
              "border-color-l"
            ],
            "border-color-x": [
              "border-color-r",
              "border-color-l"
            ],
            "border-color-y": [
              "border-color-t",
              "border-color-b"
            ],
            translate: [
              "translate-x",
              "translate-y",
              "translate-none"
            ],
            "translate-none": [
              "translate",
              "translate-x",
              "translate-y",
              "translate-z"
            ],
            "scroll-m": [
              "scroll-mx",
              "scroll-my",
              "scroll-ms",
              "scroll-me",
              "scroll-mt",
              "scroll-mr",
              "scroll-mb",
              "scroll-ml"
            ],
            "scroll-mx": [
              "scroll-mr",
              "scroll-ml"
            ],
            "scroll-my": [
              "scroll-mt",
              "scroll-mb"
            ],
            "scroll-p": [
              "scroll-px",
              "scroll-py",
              "scroll-ps",
              "scroll-pe",
              "scroll-pt",
              "scroll-pr",
              "scroll-pb",
              "scroll-pl"
            ],
            "scroll-px": [
              "scroll-pr",
              "scroll-pl"
            ],
            "scroll-py": [
              "scroll-pt",
              "scroll-pb"
            ],
            touch: [
              "touch-x",
              "touch-y",
              "touch-pz"
            ],
            "touch-x": [
              "touch"
            ],
            "touch-y": [
              "touch"
            ],
            "touch-pz": [
              "touch"
            ]
          },
          conflictingClassGroupModifiers: {
            "font-size": [
              "leading"
            ]
          },
          orderSensitiveModifiers: [
            "*",
            "**",
            "after",
            "backdrop",
            "before",
            "details-content",
            "file",
            "first-letter",
            "first-line",
            "marker",
            "placeholder",
            "selection"
          ]
        };
      }, z0 = d0(O0);
      function Kr(...l) {
        return z0(Jm(l));
      }
      function k0(l, a, { checkForDefaultPrevented: o = true } = {}) {
        return function(u) {
          if (l == null ? void 0 : l(u), o === false || !u.defaultPrevented) return a == null ? void 0 : a(u);
        };
      }
      function M0(l, a = []) {
        let o = [];
        function i(c, d) {
          const h = ae.createContext(d), m = o.length;
          o = [
            ...o,
            d
          ];
          const g = (_) => {
            var _a;
            const { scope: E, children: R, ...z } = _, H = ((_a = E == null ? void 0 : E[l]) == null ? void 0 : _a[m]) || h, Y = ae.useMemo(() => z, Object.values(z));
            return V.jsx(H.Provider, {
              value: Y,
              children: R
            });
          };
          g.displayName = c + "Provider";
          function p(_, E) {
            var _a;
            const R = ((_a = E == null ? void 0 : E[l]) == null ? void 0 : _a[m]) || h, z = ae.useContext(R);
            if (z) return z;
            if (d !== void 0) return d;
            throw new Error(`\`${_}\` must be used within \`${c}\``);
          }
          return [
            g,
            p
          ];
        }
        const u = () => {
          const c = o.map((d) => ae.createContext(d));
          return function(h) {
            const m = (h == null ? void 0 : h[l]) || c;
            return ae.useMemo(() => ({
              [`__scope${l}`]: {
                ...h,
                [l]: m
              }
            }), [
              h,
              m
            ]);
          };
        };
        return u.scopeName = l, [
          i,
          L0(u, ...a)
        ];
      }
      function L0(...l) {
        const a = l[0];
        if (l.length === 1) return a;
        const o = () => {
          const i = l.map((u) => ({
            useScope: u(),
            scopeName: u.scopeName
          }));
          return function(c) {
            const d = i.reduce((h, { useScope: m, scopeName: g }) => {
              const _ = m(c)[`__scope${g}`];
              return {
                ...h,
                ..._
              };
            }, {});
            return ae.useMemo(() => ({
              [`__scope${a.scopeName}`]: d
            }), [
              d
            ]);
          };
        };
        return o.scopeName = a.scopeName, o;
      }
      var Xr = (globalThis == null ? void 0 : globalThis.document) ? ae.useLayoutEffect : () => {
      }, G0 = Qm[" useInsertionEffect ".trim().toString()] || Xr;
      function U0({ prop: l, defaultProp: a, onChange: o = () => {
      }, caller: i }) {
        const [u, c, d] = j0({
          defaultProp: a,
          onChange: o
        }), h = l !== void 0, m = h ? l : u;
        {
          const p = ae.useRef(l !== void 0);
          ae.useEffect(() => {
            const _ = p.current;
            _ !== h && console.warn(`${i} is changing from ${_ ? "controlled" : "uncontrolled"} to ${h ? "controlled" : "uncontrolled"}. Components should not switch from controlled to uncontrolled (or vice versa). Decide between using a controlled or uncontrolled value for the lifetime of the component.`), p.current = h;
          }, [
            h,
            i
          ]);
        }
        const g = ae.useCallback((p) => {
          var _a;
          if (h) {
            const _ = B0(p) ? p(l) : p;
            _ !== l && ((_a = d.current) == null ? void 0 : _a.call(d, _));
          } else c(p);
        }, [
          h,
          l,
          c,
          d
        ]);
        return [
          m,
          g
        ];
      }
      function j0({ defaultProp: l, onChange: a }) {
        const [o, i] = ae.useState(l), u = ae.useRef(o), c = ae.useRef(a);
        return G0(() => {
          c.current = a;
        }, [
          a
        ]), ae.useEffect(() => {
          var _a;
          u.current !== o && ((_a = c.current) == null ? void 0 : _a.call(c, o), u.current = o);
        }, [
          o,
          u
        ]), [
          o,
          i,
          c
        ];
      }
      function B0(l) {
        return typeof l == "function";
      }
      function im(l, a) {
        if (typeof l == "function") return l(a);
        l != null && (l.current = a);
      }
      function cv(...l) {
        return (a) => {
          let o = false;
          const i = l.map((u) => {
            const c = im(u, a);
            return !o && typeof c == "function" && (o = true), c;
          });
          if (o) return () => {
            for (let u = 0; u < i.length; u++) {
              const c = i[u];
              typeof c == "function" ? c() : im(l[u], null);
            }
          };
        };
      }
      function fv(...l) {
        return ae.useCallback(cv(...l), l);
      }
      Km();
      function dv(l) {
        const a = F0(l), o = ae.forwardRef((i, u) => {
          const { children: c, ...d } = i, h = ae.Children.toArray(c), m = h.find(V0);
          if (m) {
            const g = m.props.children, p = h.map((_) => _ === m ? ae.Children.count(g) > 1 ? ae.Children.only(null) : ae.isValidElement(g) ? g.props.children : null : _);
            return V.jsx(a, {
              ...d,
              ref: u,
              children: ae.isValidElement(g) ? ae.cloneElement(g, void 0, p) : null
            });
          }
          return V.jsx(a, {
            ...d,
            ref: u,
            children: c
          });
        });
        return o.displayName = `${l}.Slot`, o;
      }
      var H0 = dv("Slot");
      function F0(l) {
        const a = ae.forwardRef((o, i) => {
          const { children: u, ...c } = o;
          if (ae.isValidElement(u)) {
            const d = $0(u), h = Y0(c, u.props);
            return u.type !== ae.Fragment && (h.ref = i ? cv(i, d) : d), ae.cloneElement(u, h);
          }
          return ae.Children.count(u) > 1 ? ae.Children.only(null) : null;
        });
        return a.displayName = `${l}.SlotClone`, a;
      }
      var q0 = Symbol("radix.slottable");
      function V0(l) {
        return ae.isValidElement(l) && typeof l.type == "function" && "__radixId" in l.type && l.type.__radixId === q0;
      }
      function Y0(l, a) {
        const o = {
          ...a
        };
        for (const i in a) {
          const u = l[i], c = a[i];
          /^on[A-Z]/.test(i) ? u && c ? o[i] = (...h) => {
            const m = c(...h);
            return u(...h), m;
          } : u && (o[i] = u) : i === "style" ? o[i] = {
            ...u,
            ...c
          } : i === "className" && (o[i] = [
            u,
            c
          ].filter(Boolean).join(" "));
        }
        return {
          ...l,
          ...o
        };
      }
      function $0(l) {
        var _a, _b;
        let a = (_a = Object.getOwnPropertyDescriptor(l.props, "ref")) == null ? void 0 : _a.get, o = a && "isReactWarning" in a && a.isReactWarning;
        return o ? l.ref : (a = (_b = Object.getOwnPropertyDescriptor(l, "ref")) == null ? void 0 : _b.get, o = a && "isReactWarning" in a && a.isReactWarning, o ? l.props.ref : l.props.ref || l.ref);
      }
      var X0 = [
        "a",
        "button",
        "div",
        "form",
        "h2",
        "h3",
        "img",
        "input",
        "label",
        "li",
        "nav",
        "ol",
        "p",
        "select",
        "span",
        "svg",
        "ul"
      ], Bc = X0.reduce((l, a) => {
        const o = dv(`Primitive.${a}`), i = ae.forwardRef((u, c) => {
          const { asChild: d, ...h } = u, m = d ? o : a;
          return typeof window < "u" && (window[Symbol.for("radix-ui")] = true), V.jsx(m, {
            ...h,
            ref: c
          });
        });
        return i.displayName = `Primitive.${a}`, {
          ...l,
          [a]: i
        };
      }, {});
      function Z0(l, a) {
        return ae.useReducer((o, i) => a[o][i] ?? o, l);
      }
      var hv = (l) => {
        const { present: a, children: o } = l, i = Q0(a), u = typeof o == "function" ? o({
          present: i.isPresent
        }) : ae.Children.only(o), c = fv(i.ref, K0(u));
        return typeof o == "function" || i.isPresent ? ae.cloneElement(u, {
          ref: c
        }) : null;
      };
      hv.displayName = "Presence";
      function Q0(l) {
        const [a, o] = ae.useState(), i = ae.useRef(null), u = ae.useRef(l), c = ae.useRef("none"), d = l ? "mounted" : "unmounted", [h, m] = Z0(d, {
          mounted: {
            UNMOUNT: "unmounted",
            ANIMATION_OUT: "unmountSuspended"
          },
          unmountSuspended: {
            MOUNT: "mounted",
            ANIMATION_END: "unmounted"
          },
          unmounted: {
            MOUNT: "mounted"
          }
        });
        return ae.useEffect(() => {
          const g = ho(i.current);
          c.current = h === "mounted" ? g : "none";
        }, [
          h
        ]), Xr(() => {
          const g = i.current, p = u.current;
          if (p !== l) {
            const E = c.current, R = ho(g);
            l ? m("MOUNT") : R === "none" || (g == null ? void 0 : g.display) === "none" ? m("UNMOUNT") : m(p && E !== R ? "ANIMATION_OUT" : "UNMOUNT"), u.current = l;
          }
        }, [
          l,
          m
        ]), Xr(() => {
          if (a) {
            let g;
            const p = a.ownerDocument.defaultView ?? window, _ = (R) => {
              const H = ho(i.current).includes(R.animationName);
              if (R.target === a && H && (m("ANIMATION_END"), !u.current)) {
                const Y = a.style.animationFillMode;
                a.style.animationFillMode = "forwards", g = p.setTimeout(() => {
                  a.style.animationFillMode === "forwards" && (a.style.animationFillMode = Y);
                });
              }
            }, E = (R) => {
              R.target === a && (c.current = ho(i.current));
            };
            return a.addEventListener("animationstart", E), a.addEventListener("animationcancel", _), a.addEventListener("animationend", _), () => {
              p.clearTimeout(g), a.removeEventListener("animationstart", E), a.removeEventListener("animationcancel", _), a.removeEventListener("animationend", _);
            };
          } else m("ANIMATION_END");
        }, [
          a,
          m
        ]), {
          isPresent: [
            "mounted",
            "unmountSuspended"
          ].includes(h),
          ref: ae.useCallback((g) => {
            i.current = g ? getComputedStyle(g) : null, o(g);
          }, [])
        };
      }
      function ho(l) {
        return (l == null ? void 0 : l.animationName) || "none";
      }
      function K0(l) {
        var _a, _b;
        let a = (_a = Object.getOwnPropertyDescriptor(l.props, "ref")) == null ? void 0 : _a.get, o = a && "isReactWarning" in a && a.isReactWarning;
        return o ? l.ref : (a = (_b = Object.getOwnPropertyDescriptor(l, "ref")) == null ? void 0 : _b.get, o = a && "isReactWarning" in a && a.isReactWarning, o ? l.props.ref : l.props.ref || l.ref);
      }
      var P0 = Qm[" useId ".trim().toString()] || (() => {
      }), I0 = 0;
      function W0(l) {
        const [a, o] = ae.useState(P0());
        return Xr(() => {
          o((i) => i ?? String(I0++));
        }, [
          l
        ]), l || (a ? `radix-${a}` : "");
      }
      var Do = "Collapsible", [J0, N1] = M0(Do), [ew, Hc] = J0(Do), gv = ae.forwardRef((l, a) => {
        const { __scopeCollapsible: o, open: i, defaultOpen: u, disabled: c, onOpenChange: d, ...h } = l, [m, g] = U0({
          prop: i,
          defaultProp: u ?? false,
          onChange: d,
          caller: Do
        });
        return V.jsx(ew, {
          scope: o,
          disabled: c,
          contentId: W0(),
          open: m,
          onOpenToggle: ae.useCallback(() => g((p) => !p), [
            g
          ]),
          children: V.jsx(Bc.div, {
            "data-state": qc(m),
            "data-disabled": c ? "" : void 0,
            ...h,
            ref: a
          })
        });
      });
      gv.displayName = Do;
      var mv = "CollapsibleTrigger", vv = ae.forwardRef((l, a) => {
        const { __scopeCollapsible: o, ...i } = l, u = Hc(mv, o);
        return V.jsx(Bc.button, {
          type: "button",
          "aria-controls": u.contentId,
          "aria-expanded": u.open || false,
          "data-state": qc(u.open),
          "data-disabled": u.disabled ? "" : void 0,
          disabled: u.disabled,
          ...i,
          ref: a,
          onClick: k0(l.onClick, u.onOpenToggle)
        });
      });
      vv.displayName = mv;
      var Fc = "CollapsibleContent", pv = ae.forwardRef((l, a) => {
        const { forceMount: o, ...i } = l, u = Hc(Fc, l.__scopeCollapsible);
        return V.jsx(hv, {
          present: o || u.open,
          children: ({ present: c }) => V.jsx(tw, {
            ...i,
            ref: a,
            present: c
          })
        });
      });
      pv.displayName = Fc;
      var tw = ae.forwardRef((l, a) => {
        const { __scopeCollapsible: o, present: i, children: u, ...c } = l, d = Hc(Fc, o), [h, m] = ae.useState(i), g = ae.useRef(null), p = fv(a, g), _ = ae.useRef(0), E = _.current, R = ae.useRef(0), z = R.current, H = d.open || h, Y = ae.useRef(H), te = ae.useRef(void 0);
        return ae.useEffect(() => {
          const J = requestAnimationFrame(() => Y.current = false);
          return () => cancelAnimationFrame(J);
        }, []), Xr(() => {
          const J = g.current;
          if (J) {
            te.current = te.current || {
              transitionDuration: J.style.transitionDuration,
              animationName: J.style.animationName
            }, J.style.transitionDuration = "0s", J.style.animationName = "none";
            const I = J.getBoundingClientRect();
            _.current = I.height, R.current = I.width, Y.current || (J.style.transitionDuration = te.current.transitionDuration, J.style.animationName = te.current.animationName), m(i);
          }
        }, [
          d.open,
          i
        ]), V.jsx(Bc.div, {
          "data-state": qc(d.open),
          "data-disabled": d.disabled ? "" : void 0,
          id: d.contentId,
          hidden: !H,
          ...c,
          ref: p,
          style: {
            "--radix-collapsible-content-height": E ? `${E}px` : void 0,
            "--radix-collapsible-content-width": z ? `${z}px` : void 0,
            ...l.style
          },
          children: H && u
        });
      });
      function qc(l) {
        return l ? "open" : "closed";
      }
      var nw = gv;
      function ec({ ...l }) {
        return V.jsx(nw, {
          "data-slot": "collapsible",
          ...l
        });
      }
      function tc({ ...l }) {
        return V.jsx(vv, {
          "data-slot": "collapsible-trigger",
          ...l
        });
      }
      function nc({ ...l }) {
        return V.jsx(pv, {
          "data-slot": "collapsible-content",
          ...l
        });
      }
      const Vc = xn.forwardRef(({ className: l, ...a }, o) => V.jsx("button", {
        className: Kr("inline-flex items-center justify-center whitespace-nowrap rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 bg-primary text-primary-foreground hover:bg-primary/90 h-10 px-4 py-2", l),
        ref: o,
        ...a
      })), ht = xn.forwardRef(({ className: l, type: a, ...o }, i) => V.jsx("input", {
        type: a,
        className: Kr("flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50", l),
        ref: i,
        ...o
      })), st = xn.forwardRef(({ className: l, ...a }, o) => V.jsx("label", {
        ref: o,
        className: Kr("text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70", l),
        ...a
      })), iw = xn.forwardRef(({ className: l, ...a }, o) => V.jsx("select", {
        className: Kr("flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50", l),
        ref: o,
        ...a
      })), aw = ({ onSubmit: l, config: a }) => {
        const o = (i) => {
          i.preventDefault();
          const u = new FormData(i.currentTarget), c = {
            active_view_capacity: Number(u.get("active_view_capacity")),
            passive_view_capacity: Number(u.get("passive_view_capacity")),
            active_random_walk_length: Number(u.get("active_random_walk_length")),
            passive_random_walk_length: Number(u.get("passive_random_walk_length")),
            shuffle_random_walk_length: Number(u.get("shuffle_random_walk_length")),
            shuffle_active_view_count: Number(u.get("shuffle_active_view_count")),
            shuffle_passive_view_count: Number(u.get("shuffle_passive_view_count")),
            shuffle_interval: u.get("shuffle_interval"),
            neighbor_request_timeout: u.get("neighbor_request_timeout")
          };
          l(c);
        };
        return V.jsxs("form", {
          onSubmit: o,
          className: "space-y-4",
          children: [
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "active_view_capacity",
                  children: "Active View Capacity"
                }),
                V.jsx(ht, {
                  id: "active_view_capacity",
                  name: "active_view_capacity",
                  type: "number",
                  defaultValue: a.active_view_capacity
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "passive_view_capacity",
                  children: "Passive View Capacity"
                }),
                V.jsx(ht, {
                  id: "passive_view_capacity",
                  name: "passive_view_capacity",
                  type: "number",
                  defaultValue: a.passive_view_capacity
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "active_random_walk_length",
                  children: "Active Random Walk Length"
                }),
                V.jsx(ht, {
                  id: "active_random_walk_length",
                  name: "active_random_walk_length",
                  type: "number",
                  defaultValue: a.active_random_walk_length
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "passive_random_walk_length",
                  children: "Passive Random Walk Length"
                }),
                V.jsx(ht, {
                  id: "passive_random_walk_length",
                  name: "passive_random_walk_length",
                  type: "number",
                  defaultValue: a.passive_random_walk_length
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "shuffle_random_walk_length",
                  children: "Shuffle Random Walk Length"
                }),
                V.jsx(ht, {
                  id: "shuffle_random_walk_length",
                  name: "shuffle_random_walk_length",
                  type: "number",
                  defaultValue: a.shuffle_random_walk_length
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "shuffle_active_view_count",
                  children: "Shuffle Active View Count"
                }),
                V.jsx(ht, {
                  id: "shuffle_active_view_count",
                  name: "shuffle_active_view_count",
                  type: "number",
                  defaultValue: a.shuffle_active_view_count
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "shuffle_passive_view_count",
                  children: "Shuffle Passive View Count"
                }),
                V.jsx(ht, {
                  id: "shuffle_passive_view_count",
                  name: "shuffle_passive_view_count",
                  type: "number",
                  defaultValue: a.shuffle_passive_view_count
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "shuffle_interval",
                  children: "Shuffle Interval (ms)"
                }),
                V.jsx(ht, {
                  id: "shuffle_interval",
                  name: "shuffle_interval",
                  type: "text",
                  defaultValue: a.shuffle_interval
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "neighbor_request_timeout",
                  children: "Neighbor Request Timeout (ms)"
                }),
                V.jsx(ht, {
                  id: "neighbor_request_timeout",
                  name: "neighbor_request_timeout",
                  type: "text",
                  defaultValue: a.neighbor_request_timeout
                })
              ]
            }),
            V.jsx(Vc, {
              type: "submit",
              className: "w-full",
              children: "Update Swarm Config"
            })
          ]
        });
      }, rw = ({ onSubmit: l, config: a = {} }) => {
        const o = (i) => {
          i.preventDefault();
          const u = new FormData(i.currentTarget), c = {
            graft_timeout_1: u.get("graft_timeout_1"),
            graft_timeout_2: u.get("graft_timeout_2"),
            dispatch_timeout: u.get("dispatch_timeout"),
            optimization_threshold: Number(u.get("optimization_threshold")),
            message_cache_retention: u.get("message_cache_retention"),
            message_id_retention: u.get("message_id_retention"),
            cache_evict_interval: u.get("cache_evict_interval")
          };
          l(c);
        };
        return V.jsxs("form", {
          onSubmit: o,
          className: "space-y-4",
          children: [
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "graft_timeout_1",
                  children: "Graft Timeout 1 (ms)"
                }),
                V.jsx(ht, {
                  id: "graft_timeout_1",
                  name: "graft_timeout_1",
                  type: "text",
                  defaultValue: a.graft_timeout_1
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "graft_timeout_2",
                  children: "Graft Timeout 2 (ms)"
                }),
                V.jsx(ht, {
                  id: "graft_timeout_2",
                  name: "graft_timeout_2",
                  type: "text",
                  defaultValue: a.graft_timeout_2
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "dispatch_timeout",
                  children: "Dispatch Timeout (ms)"
                }),
                V.jsx(ht, {
                  id: "dispatch_timeout",
                  name: "dispatch_timeout",
                  type: "text",
                  defaultValue: a.dispatch_timeout
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "optimization_threshold",
                  children: "Optimization Threshold"
                }),
                V.jsx(ht, {
                  id: "optimization_threshold",
                  name: "optimization_threshold",
                  type: "number",
                  defaultValue: a.optimization_threshold
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "message_cache_retention",
                  children: "Message Cache Retention (ms)"
                }),
                V.jsx(ht, {
                  id: "message_cache_retention",
                  name: "message_cache_retention",
                  type: "text",
                  defaultValue: a.message_cache_retention
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "message_id_retention",
                  children: "Message ID Retention (ms)"
                }),
                V.jsx(ht, {
                  id: "message_id_retention",
                  name: "message_id_retention",
                  type: "text",
                  defaultValue: a.message_id_retention
                })
              ]
            }),
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "cache_evict_interval",
                  children: "Cache Evict Interval (ms)"
                }),
                V.jsx(ht, {
                  id: "cache_evict_interval",
                  name: "cache_evict_interval",
                  type: "text",
                  defaultValue: a.cache_evict_interval
                })
              ]
            }),
            V.jsx(Vc, {
              type: "submit",
              className: "w-full",
              children: "Update Gossip Config"
            })
          ]
        });
      }, lw = ({ onSubmit: l, config: a }) => {
        const [o, i] = xn.useState(a.type), u = (c) => {
          c.preventDefault();
          const d = new FormData(c.currentTarget);
          let h;
          o === "static" ? h = {
            type: "static",
            duration: d.get("duration")
          } : h = {
            type: "dynamic",
            min: d.get("min"),
            max: d.get("max")
          }, l(h);
        };
        return V.jsxs("form", {
          onSubmit: u,
          className: "space-y-4",
          children: [
            V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "latency_type",
                  children: "Latency Type"
                }),
                V.jsxs(iw, {
                  id: "latency_type",
                  name: "latency_type",
                  value: o,
                  onChange: (c) => i(c.target.value),
                  children: [
                    V.jsx("option", {
                      value: "static",
                      children: "Static"
                    }),
                    V.jsx("option", {
                      value: "dynamic",
                      children: "Dynamic"
                    })
                  ]
                })
              ]
            }),
            o === "static" ? V.jsxs("div", {
              className: "space-y-2",
              children: [
                V.jsx(st, {
                  htmlFor: "duration",
                  children: "Duration (ms)"
                }),
                V.jsx(ht, {
                  id: "duration",
                  name: "duration",
                  type: "text",
                  defaultValue: a.type === "static" ? a.duration : "100ms"
                })
              ]
            }) : V.jsxs(V.Fragment, {
              children: [
                V.jsxs("div", {
                  className: "space-y-2",
                  children: [
                    V.jsx(st, {
                      htmlFor: "min",
                      children: "Min Duration (ms)"
                    }),
                    V.jsx(ht, {
                      id: "min",
                      name: "min",
                      type: "text",
                      defaultValue: a.type === "dynamic" ? a.min : "50ms"
                    })
                  ]
                }),
                V.jsxs("div", {
                  className: "space-y-2",
                  children: [
                    V.jsx(st, {
                      htmlFor: "max",
                      children: "Max Duration (ms)"
                    }),
                    V.jsx(ht, {
                      id: "max",
                      name: "max",
                      type: "text",
                      defaultValue: a.type === "dynamic" ? a.max : "200ms"
                    })
                  ]
                })
              ]
            }),
            V.jsx(Vc, {
              type: "submit",
              className: "w-full",
              children: "Update Latency Config"
            })
          ]
        });
      };
      function ow({ config: l, setConfig: a }) {
        const o = (c) => {
          l = {
            ...l,
            proto: {
              ...l.proto,
              membership: c
            }
          }, a(l);
        }, i = (c) => {
          l = {
            ...l,
            proto: {
              ...l.proto,
              broadcast: c
            }
          }, a(l);
        }, u = (c) => {
          l = {
            ...l,
            latency: c
          }, a(l);
        };
        return V.jsx("div", {
          children: V.jsxs("div", {
            className: "space-y-6",
            children: [
              V.jsxs(ec, {
                children: [
                  V.jsx(tc, {
                    children: V.jsx("h3", {
                      className: "text-lg font-semibold",
                      children: "Swarm Configuration"
                    })
                  }),
                  V.jsx(nc, {
                    children: V.jsx(aw, {
                      onSubmit: o,
                      config: l.proto.membership
                    })
                  })
                ]
              }),
              V.jsxs(ec, {
                children: [
                  V.jsx(tc, {
                    children: V.jsx("h3", {
                      className: "text-lg font-semibold",
                      children: "Gossip Configuration"
                    })
                  }),
                  V.jsx(nc, {
                    children: V.jsx(rw, {
                      onSubmit: i,
                      config: l.proto.broadcast
                    })
                  })
                ]
              }),
              V.jsxs(ec, {
                children: [
                  V.jsx(tc, {
                    children: V.jsx("h3", {
                      className: "text-lg font-semibold",
                      children: "Latency Configuration"
                    })
                  }),
                  V.jsx(nc, {
                    children: V.jsx(lw, {
                      onSubmit: u,
                      config: l.latency
                    })
                  })
                ]
              })
            ]
          })
        });
      }
      var So = {
        exports: {}
      };
      var uw = So.exports, am;
      function sw() {
        return am || (am = 1, function(l, a) {
          ((o, i) => {
            l.exports = i();
          })(uw, function o() {
            var i = typeof self < "u" ? self : typeof window < "u" ? window : i !== void 0 ? i : {}, u, c = !i.document && !!i.postMessage, d = i.IS_PAPA_WORKER || false, h = {}, m = 0, g = {};
            function p(y) {
              this._handle = null, this._finished = false, this._completed = false, this._halted = false, this._input = null, this._baseIndex = 0, this._partialLine = "", this._rowCount = 0, this._start = 0, this._nextChunk = null, this.isFirstChunk = true, this._completeResults = {
                data: [],
                errors: [],
                meta: {}
              }, (function(w) {
                var S = B(w);
                S.chunkSize = parseInt(S.chunkSize), w.step || w.chunk || (S.chunkSize = null), this._handle = new H(S), (this._handle.streamer = this)._config = S;
              }).call(this, y), this.parseChunk = function(w, S) {
                var N = parseInt(this._config.skipFirstNLines) || 0;
                if (this.isFirstChunk && 0 < N) {
                  let re = this._config.newline;
                  re || (L = this._config.quoteChar || '"', re = this._handle.guessLineEndings(w, L)), w = [
                    ...w.split(re).slice(N)
                  ].join(re);
                }
                this.isFirstChunk && T(this._config.beforeFirstChunk) && (L = this._config.beforeFirstChunk(w)) !== void 0 && (w = L), this.isFirstChunk = false, this._halted = false;
                var N = this._partialLine + w, L = (this._partialLine = "", this._handle.parse(N, this._baseIndex, !this._finished));
                if (!this._handle.paused() && !this._handle.aborted()) {
                  if (w = L.meta.cursor, N = (this._finished || (this._partialLine = N.substring(w - this._baseIndex), this._baseIndex = w), L && L.data && (this._rowCount += L.data.length), this._finished || this._config.preview && this._rowCount >= this._config.preview), d) i.postMessage({
                    results: L,
                    workerId: g.WORKER_ID,
                    finished: N
                  });
                  else if (T(this._config.chunk) && !S) {
                    if (this._config.chunk(L, this._handle), this._handle.paused() || this._handle.aborted()) return void (this._halted = true);
                    this._completeResults = L = void 0;
                  }
                  return this._config.step || this._config.chunk || (this._completeResults.data = this._completeResults.data.concat(L.data), this._completeResults.errors = this._completeResults.errors.concat(L.errors), this._completeResults.meta = L.meta), this._completed || !N || !T(this._config.complete) || L && L.meta.aborted || (this._config.complete(this._completeResults, this._input), this._completed = true), N || L && L.meta.paused || this._nextChunk(), L;
                }
                this._halted = true;
              }, this._sendError = function(w) {
                T(this._config.error) ? this._config.error(w) : d && this._config.error && i.postMessage({
                  workerId: g.WORKER_ID,
                  error: w,
                  finished: false
                });
              };
            }
            function _(y) {
              var w;
              (y = y || {}).chunkSize || (y.chunkSize = g.RemoteChunkSize), p.call(this, y), this._nextChunk = c ? function() {
                this._readChunk(), this._chunkLoaded();
              } : function() {
                this._readChunk();
              }, this.stream = function(S) {
                this._input = S, this._nextChunk();
              }, this._readChunk = function() {
                if (this._finished) this._chunkLoaded();
                else {
                  if (w = new XMLHttpRequest(), this._config.withCredentials && (w.withCredentials = this._config.withCredentials), c || (w.onload = Z(this._chunkLoaded, this), w.onerror = Z(this._chunkError, this)), w.open(this._config.downloadRequestBody ? "POST" : "GET", this._input, !c), this._config.downloadRequestHeaders) {
                    var S, N = this._config.downloadRequestHeaders;
                    for (S in N) w.setRequestHeader(S, N[S]);
                  }
                  var L;
                  this._config.chunkSize && (L = this._start + this._config.chunkSize - 1, w.setRequestHeader("Range", "bytes=" + this._start + "-" + L));
                  try {
                    w.send(this._config.downloadRequestBody);
                  } catch (re) {
                    this._chunkError(re.message);
                  }
                  c && w.status === 0 && this._chunkError();
                }
              }, this._chunkLoaded = function() {
                w.readyState === 4 && (w.status < 200 || 400 <= w.status ? this._chunkError() : (this._start += this._config.chunkSize || w.responseText.length, this._finished = !this._config.chunkSize || this._start >= ((S) => (S = S.getResponseHeader("Content-Range")) !== null ? parseInt(S.substring(S.lastIndexOf("/") + 1)) : -1)(w), this.parseChunk(w.responseText)));
              }, this._chunkError = function(S) {
                S = w.statusText || S, this._sendError(new Error(S));
              };
            }
            function E(y) {
              (y = y || {}).chunkSize || (y.chunkSize = g.LocalChunkSize), p.call(this, y);
              var w, S, N = typeof FileReader < "u";
              this.stream = function(L) {
                this._input = L, S = L.slice || L.webkitSlice || L.mozSlice, N ? ((w = new FileReader()).onload = Z(this._chunkLoaded, this), w.onerror = Z(this._chunkError, this)) : w = new FileReaderSync(), this._nextChunk();
              }, this._nextChunk = function() {
                this._finished || this._config.preview && !(this._rowCount < this._config.preview) || this._readChunk();
              }, this._readChunk = function() {
                var L = this._input, re = (this._config.chunkSize && (re = Math.min(this._start + this._config.chunkSize, this._input.size), L = S.call(L, this._start, re)), w.readAsText(L, this._config.encoding));
                N || this._chunkLoaded({
                  target: {
                    result: re
                  }
                });
              }, this._chunkLoaded = function(L) {
                this._start += this._config.chunkSize, this._finished = !this._config.chunkSize || this._start >= this._input.size, this.parseChunk(L.target.result);
              }, this._chunkError = function() {
                this._sendError(w.error);
              };
            }
            function R(y) {
              var w;
              p.call(this, y = y || {}), this.stream = function(S) {
                return w = S, this._nextChunk();
              }, this._nextChunk = function() {
                var S, N;
                if (!this._finished) return S = this._config.chunkSize, w = S ? (N = w.substring(0, S), w.substring(S)) : (N = w, ""), this._finished = !w, this.parseChunk(N);
              };
            }
            function z(y) {
              p.call(this, y = y || {});
              var w = [], S = true, N = false;
              this.pause = function() {
                p.prototype.pause.apply(this, arguments), this._input.pause();
              }, this.resume = function() {
                p.prototype.resume.apply(this, arguments), this._input.resume();
              }, this.stream = function(L) {
                this._input = L, this._input.on("data", this._streamData), this._input.on("end", this._streamEnd), this._input.on("error", this._streamError);
              }, this._checkIsFinished = function() {
                N && w.length === 1 && (this._finished = true);
              }, this._nextChunk = function() {
                this._checkIsFinished(), w.length ? this.parseChunk(w.shift()) : S = true;
              }, this._streamData = Z(function(L) {
                try {
                  w.push(typeof L == "string" ? L : L.toString(this._config.encoding)), S && (S = false, this._checkIsFinished(), this.parseChunk(w.shift()));
                } catch (re) {
                  this._streamError(re);
                }
              }, this), this._streamError = Z(function(L) {
                this._streamCleanUp(), this._sendError(L);
              }, this), this._streamEnd = Z(function() {
                this._streamCleanUp(), N = true, this._streamData("");
              }, this), this._streamCleanUp = Z(function() {
                this._input.removeListener("data", this._streamData), this._input.removeListener("end", this._streamEnd), this._input.removeListener("error", this._streamError);
              }, this);
            }
            function H(y) {
              var w, S, N, L, re = Math.pow(2, 53), ue = -re, oe = /^\s*-?(\d+\.?|\.\d+|\d+\.\d+)([eE][-+]?\d+)?\s*$/, D = /^((\d{4}-[01]\d-[0-3]\dT[0-2]\d:[0-5]\d:[0-5]\d\.\d+([+-][0-2]\d:[0-5]\d|Z))|(\d{4}-[01]\d-[0-3]\dT[0-2]\d:[0-5]\d:[0-5]\d([+-][0-2]\d:[0-5]\d|Z))|(\d{4}-[01]\d-[0-3]\dT[0-2]\d:[0-5]\d([+-][0-2]\d:[0-5]\d|Z)))$/, X = this, $ = 0, ne = 0, x = false, k = false, K = [], U = {
                data: [],
                errors: [],
                meta: {}
              };
              function ie(fe) {
                return y.skipEmptyLines === "greedy" ? fe.join("").trim() === "" : fe.length === 1 && fe[0].length === 0;
              }
              function de() {
                if (U && N && (be("Delimiter", "UndetectableDelimiter", "Unable to auto-detect delimiting character; defaulted to '" + g.DefaultDelimiter + "'"), N = false), y.skipEmptyLines && (U.data = U.data.filter(function(pe) {
                  return !ie(pe);
                })), ce()) {
                  let pe = function(Me, Ze) {
                    T(y.transformHeader) && (Me = y.transformHeader(Me, Ze)), K.push(Me);
                  };
                  if (U) if (Array.isArray(U.data[0])) {
                    for (var fe = 0; ce() && fe < U.data.length; fe++) U.data[fe].forEach(pe);
                    U.data.splice(0, 1);
                  } else U.data.forEach(pe);
                }
                function _e(pe, Me) {
                  for (var Ze = y.header ? {} : [], Le = 0; Le < pe.length; Le++) {
                    var je = Le, Ae = pe[Le], Ae = ((nt, Te) => ((Ke) => (y.dynamicTypingFunction && y.dynamicTyping[Ke] === void 0 && (y.dynamicTyping[Ke] = y.dynamicTypingFunction(Ke)), (y.dynamicTyping[Ke] || y.dynamicTyping) === true))(nt) ? Te === "true" || Te === "TRUE" || Te !== "false" && Te !== "FALSE" && (((Ke) => {
                      if (oe.test(Ke) && (Ke = parseFloat(Ke), ue < Ke && Ke < re)) return 1;
                    })(Te) ? parseFloat(Te) : D.test(Te) ? new Date(Te) : Te === "" ? null : Te) : Te)(je = y.header ? Le >= K.length ? "__parsed_extra" : K[Le] : je, Ae = y.transform ? y.transform(Ae, je) : Ae);
                    je === "__parsed_extra" ? (Ze[je] = Ze[je] || [], Ze[je].push(Ae)) : Ze[je] = Ae;
                  }
                  return y.header && (Le > K.length ? be("FieldMismatch", "TooManyFields", "Too many fields: expected " + K.length + " fields but parsed " + Le, ne + Me) : Le < K.length && be("FieldMismatch", "TooFewFields", "Too few fields: expected " + K.length + " fields but parsed " + Le, ne + Me)), Ze;
                }
                var ke;
                U && (y.header || y.dynamicTyping || y.transform) && (ke = 1, !U.data.length || Array.isArray(U.data[0]) ? (U.data = U.data.map(_e), ke = U.data.length) : U.data = _e(U.data, 0), y.header && U.meta && (U.meta.fields = K), ne += ke);
              }
              function ce() {
                return y.header && K.length === 0;
              }
              function be(fe, _e, ke, pe) {
                fe = {
                  type: fe,
                  code: _e,
                  message: ke
                }, pe !== void 0 && (fe.row = pe), U.errors.push(fe);
              }
              T(y.step) && (L = y.step, y.step = function(fe) {
                U = fe, ce() ? de() : (de(), U.data.length !== 0 && ($ += fe.data.length, y.preview && $ > y.preview ? S.abort() : (U.data = U.data[0], L(U, X))));
              }), this.parse = function(fe, _e, ke) {
                var pe = y.quoteChar || '"', pe = (y.newline || (y.newline = this.guessLineEndings(fe, pe)), N = false, y.delimiter ? T(y.delimiter) && (y.delimiter = y.delimiter(fe), U.meta.delimiter = y.delimiter) : ((pe = ((Me, Ze, Le, je, Ae) => {
                  var nt, Te, Ke, un;
                  Ae = Ae || [
                    ",",
                    "	",
                    "|",
                    ";",
                    g.RECORD_SEP,
                    g.UNIT_SEP
                  ];
                  for (var sn = 0; sn < Ae.length; sn++) {
                    for (var $t, Xn = Ae[sn], mt = 0, Xt = 0, Je = 0, Ve = (Ke = void 0, new te({
                      comments: je,
                      delimiter: Xn,
                      newline: Ze,
                      preview: 10
                    }).parse(Me)), vt = 0; vt < Ve.data.length; vt++) Le && ie(Ve.data[vt]) ? Je++ : ($t = Ve.data[vt].length, Xt += $t, Ke === void 0 ? Ke = $t : 0 < $t && (mt += Math.abs($t - Ke), Ke = $t));
                    0 < Ve.data.length && (Xt /= Ve.data.length - Je), (Te === void 0 || mt <= Te) && (un === void 0 || un < Xt) && 1.99 < Xt && (Te = mt, nt = Xn, un = Xt);
                  }
                  return {
                    successful: !!(y.delimiter = nt),
                    bestDelimiter: nt
                  };
                })(fe, y.newline, y.skipEmptyLines, y.comments, y.delimitersToGuess)).successful ? y.delimiter = pe.bestDelimiter : (N = true, y.delimiter = g.DefaultDelimiter), U.meta.delimiter = y.delimiter), B(y));
                return y.preview && y.header && pe.preview++, w = fe, S = new te(pe), U = S.parse(w, _e, ke), de(), x ? {
                  meta: {
                    paused: true
                  }
                } : U || {
                  meta: {
                    paused: false
                  }
                };
              }, this.paused = function() {
                return x;
              }, this.pause = function() {
                x = true, S.abort(), w = T(y.chunk) ? "" : w.substring(S.getCharIndex());
              }, this.resume = function() {
                X.streamer._halted ? (x = false, X.streamer.parseChunk(w, true)) : setTimeout(X.resume, 3);
              }, this.aborted = function() {
                return k;
              }, this.abort = function() {
                k = true, S.abort(), U.meta.aborted = true, T(y.complete) && y.complete(U), w = "";
              }, this.guessLineEndings = function(Me, pe) {
                Me = Me.substring(0, 1048576);
                var pe = new RegExp(Y(pe) + "([^]*?)" + Y(pe), "gm"), ke = (Me = Me.replace(pe, "")).split("\r"), pe = Me.split(`
`), Me = 1 < pe.length && pe[0].length < ke[0].length;
                if (ke.length === 1 || Me) return `
`;
                for (var Ze = 0, Le = 0; Le < ke.length; Le++) ke[Le][0] === `
` && Ze++;
                return Ze >= ke.length / 2 ? `\r
` : "\r";
              };
            }
            function Y(y) {
              return y.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
            }
            function te(y) {
              var w = (y = y || {}).delimiter, S = y.newline, N = y.comments, L = y.step, re = y.preview, ue = y.fastMode, oe = null, D = false, X = y.quoteChar == null ? '"' : y.quoteChar, $ = X;
              if (y.escapeChar !== void 0 && ($ = y.escapeChar), (typeof w != "string" || -1 < g.BAD_DELIMITERS.indexOf(w)) && (w = ","), N === w) throw new Error("Comment character same as delimiter");
              N === true ? N = "#" : (typeof N != "string" || -1 < g.BAD_DELIMITERS.indexOf(N)) && (N = false), S !== `
` && S !== "\r" && S !== `\r
` && (S = `
`);
              var ne = 0, x = false;
              this.parse = function(k, K, U) {
                if (typeof k != "string") throw new Error("Input must be a string");
                var ie = k.length, de = w.length, ce = S.length, be = N.length, fe = T(L), _e = [], ke = [], pe = [], Me = ne = 0;
                if (!k) return mt();
                if (ue || ue !== false && k.indexOf(X) === -1) {
                  for (var Ze = k.split(S), Le = 0; Le < Ze.length; Le++) {
                    if (pe = Ze[Le], ne += pe.length, Le !== Ze.length - 1) ne += S.length;
                    else if (U) return mt();
                    if (!N || pe.substring(0, be) !== N) {
                      if (fe) {
                        if (_e = [], un(pe.split(w)), Xt(), x) return mt();
                      } else un(pe.split(w));
                      if (re && re <= Le) return _e = _e.slice(0, re), mt(true);
                    }
                  }
                  return mt();
                }
                for (var je = k.indexOf(w, ne), Ae = k.indexOf(S, ne), nt = new RegExp(Y($) + Y(X), "g"), Te = k.indexOf(X, ne); ; ) if (k[ne] === X) for (Te = ne, ne++; ; ) {
                  if ((Te = k.indexOf(X, Te + 1)) === -1) return U || ke.push({
                    type: "Quotes",
                    code: "MissingQuotes",
                    message: "Quoted field unterminated",
                    row: _e.length,
                    index: ne
                  }), $t();
                  if (Te === ie - 1) return $t(k.substring(ne, Te).replace(nt, X));
                  if (X === $ && k[Te + 1] === $) Te++;
                  else if (X === $ || Te === 0 || k[Te - 1] !== $) {
                    je !== -1 && je < Te + 1 && (je = k.indexOf(w, Te + 1));
                    var Ke = sn((Ae = Ae !== -1 && Ae < Te + 1 ? k.indexOf(S, Te + 1) : Ae) === -1 ? je : Math.min(je, Ae));
                    if (k.substr(Te + 1 + Ke, de) === w) {
                      pe.push(k.substring(ne, Te).replace(nt, X)), k[ne = Te + 1 + Ke + de] !== X && (Te = k.indexOf(X, ne)), je = k.indexOf(w, ne), Ae = k.indexOf(S, ne);
                      break;
                    }
                    if (Ke = sn(Ae), k.substring(Te + 1 + Ke, Te + 1 + Ke + ce) === S) {
                      if (pe.push(k.substring(ne, Te).replace(nt, X)), Xn(Te + 1 + Ke + ce), je = k.indexOf(w, ne), Te = k.indexOf(X, ne), fe && (Xt(), x)) return mt();
                      if (re && _e.length >= re) return mt(true);
                      break;
                    }
                    ke.push({
                      type: "Quotes",
                      code: "InvalidQuotes",
                      message: "Trailing quote on quoted field is malformed",
                      row: _e.length,
                      index: ne
                    }), Te++;
                  }
                }
                else if (N && pe.length === 0 && k.substring(ne, ne + be) === N) {
                  if (Ae === -1) return mt();
                  ne = Ae + ce, Ae = k.indexOf(S, ne), je = k.indexOf(w, ne);
                } else if (je !== -1 && (je < Ae || Ae === -1)) pe.push(k.substring(ne, je)), ne = je + de, je = k.indexOf(w, ne);
                else {
                  if (Ae === -1) break;
                  if (pe.push(k.substring(ne, Ae)), Xn(Ae + ce), fe && (Xt(), x)) return mt();
                  if (re && _e.length >= re) return mt(true);
                }
                return $t();
                function un(Je) {
                  _e.push(Je), Me = ne;
                }
                function sn(Je) {
                  var Ve = 0;
                  return Ve = Je !== -1 && (Je = k.substring(Te + 1, Je)) && Je.trim() === "" ? Je.length : Ve;
                }
                function $t(Je) {
                  return U || (Je === void 0 && (Je = k.substring(ne)), pe.push(Je), ne = ie, un(pe), fe && Xt()), mt();
                }
                function Xn(Je) {
                  ne = Je, un(pe), pe = [], Ae = k.indexOf(S, ne);
                }
                function mt(Je) {
                  if (y.header && !K && _e.length && !D) {
                    var Ve = _e[0], vt = /* @__PURE__ */ Object.create(null), wt = new Set(Ve);
                    let Ir = false;
                    for (let Zn = 0; Zn < Ve.length; Zn++) {
                      let Zt = Ve[Zn];
                      if (vt[Zt = T(y.transformHeader) ? y.transformHeader(Zt, Zn) : Zt]) {
                        let yn, bi = vt[Zt];
                        for (; yn = Zt + "_" + bi, bi++, wt.has(yn); ) ;
                        wt.add(yn), Ve[Zn] = yn, vt[Zt]++, Ir = true, (oe = oe === null ? {} : oe)[yn] = Zt;
                      } else vt[Zt] = 1, Ve[Zn] = Zt;
                      wt.add(Zt);
                    }
                    Ir && console.warn("Duplicate headers found and renamed."), D = true;
                  }
                  return {
                    data: _e,
                    errors: ke,
                    meta: {
                      delimiter: w,
                      linebreak: S,
                      aborted: x,
                      truncated: !!Je,
                      cursor: Me + (K || 0),
                      renamedHeaders: oe
                    }
                  };
                }
                function Xt() {
                  L(mt()), _e = [], ke = [];
                }
              }, this.abort = function() {
                x = true;
              }, this.getCharIndex = function() {
                return ne;
              };
            }
            function J(y) {
              var w = y.data, S = h[w.workerId], N = false;
              if (w.error) S.userError(w.error, w.file);
              else if (w.results && w.results.data) {
                var L = {
                  abort: function() {
                    N = true, I(w.workerId, {
                      data: [],
                      errors: [],
                      meta: {
                        aborted: true
                      }
                    });
                  },
                  pause: G,
                  resume: G
                };
                if (T(S.userStep)) {
                  for (var re = 0; re < w.results.data.length && (S.userStep({
                    data: w.results.data[re],
                    errors: w.results.errors,
                    meta: w.results.meta
                  }, L), !N); re++) ;
                  delete w.results;
                } else T(S.userChunk) && (S.userChunk(w.results, L, w.file), delete w.results);
              }
              w.finished && !N && I(w.workerId, w.results);
            }
            function I(y, w) {
              var S = h[y];
              T(S.userComplete) && S.userComplete(w), S.terminate(), delete h[y];
            }
            function G() {
              throw new Error("Not implemented.");
            }
            function B(y) {
              if (typeof y != "object" || y === null) return y;
              var w, S = Array.isArray(y) ? [] : {};
              for (w in y) S[w] = B(y[w]);
              return S;
            }
            function Z(y, w) {
              return function() {
                y.apply(w, arguments);
              };
            }
            function T(y) {
              return typeof y == "function";
            }
            return g.parse = function(y, w) {
              var S = (w = w || {}).dynamicTyping || false;
              if (T(S) && (w.dynamicTypingFunction = S, S = {}), w.dynamicTyping = S, w.transform = !!T(w.transform) && w.transform, !w.worker || !g.WORKERS_SUPPORTED) return S = null, g.NODE_STREAM_INPUT, typeof y == "string" ? (y = ((N) => N.charCodeAt(0) !== 65279 ? N : N.slice(1))(y), S = new (w.download ? _ : R)(w)) : y.readable === true && T(y.read) && T(y.on) ? S = new z(w) : (i.File && y instanceof File || y instanceof Object) && (S = new E(w)), S.stream(y);
              (S = (() => {
                var N;
                return !!g.WORKERS_SUPPORTED && (N = (() => {
                  var L = i.URL || i.webkitURL || null, re = o.toString();
                  return g.BLOB_URL || (g.BLOB_URL = L.createObjectURL(new Blob([
                    "var global = (function() { if (typeof self !== 'undefined') { return self; } if (typeof window !== 'undefined') { return window; } if (typeof global !== 'undefined') { return global; } return {}; })(); global.IS_PAPA_WORKER=true; ",
                    "(",
                    re,
                    ")();"
                  ], {
                    type: "text/javascript"
                  })));
                })(), (N = new i.Worker(N)).onmessage = J, N.id = m++, h[N.id] = N);
              })()).userStep = w.step, S.userChunk = w.chunk, S.userComplete = w.complete, S.userError = w.error, w.step = T(w.step), w.chunk = T(w.chunk), w.complete = T(w.complete), w.error = T(w.error), delete w.worker, S.postMessage({
                input: y,
                config: w,
                workerId: S.id
              });
            }, g.unparse = function(y, w) {
              var S = false, N = true, L = ",", re = `\r
`, ue = '"', oe = ue + ue, D = false, X = null, $ = false, ne = ((() => {
                if (typeof w == "object") {
                  if (typeof w.delimiter != "string" || g.BAD_DELIMITERS.filter(function(K) {
                    return w.delimiter.indexOf(K) !== -1;
                  }).length || (L = w.delimiter), typeof w.quotes != "boolean" && typeof w.quotes != "function" && !Array.isArray(w.quotes) || (S = w.quotes), typeof w.skipEmptyLines != "boolean" && typeof w.skipEmptyLines != "string" || (D = w.skipEmptyLines), typeof w.newline == "string" && (re = w.newline), typeof w.quoteChar == "string" && (ue = w.quoteChar), typeof w.header == "boolean" && (N = w.header), Array.isArray(w.columns)) {
                    if (w.columns.length === 0) throw new Error("Option columns is empty");
                    X = w.columns;
                  }
                  w.escapeChar !== void 0 && (oe = w.escapeChar + ue), w.escapeFormulae instanceof RegExp ? $ = w.escapeFormulae : typeof w.escapeFormulae == "boolean" && w.escapeFormulae && ($ = /^[=+\-@\t\r].*$/);
                }
              })(), new RegExp(Y(ue), "g"));
              if (typeof y == "string" && (y = JSON.parse(y)), Array.isArray(y)) {
                if (!y.length || Array.isArray(y[0])) return x(null, y, D);
                if (typeof y[0] == "object") return x(X || Object.keys(y[0]), y, D);
              } else if (typeof y == "object") return typeof y.data == "string" && (y.data = JSON.parse(y.data)), Array.isArray(y.data) && (y.fields || (y.fields = y.meta && y.meta.fields || X), y.fields || (y.fields = Array.isArray(y.data[0]) ? y.fields : typeof y.data[0] == "object" ? Object.keys(y.data[0]) : []), Array.isArray(y.data[0]) || typeof y.data[0] == "object" || (y.data = [
                y.data
              ])), x(y.fields || [], y.data || [], D);
              throw new Error("Unable to serialize unrecognized input");
              function x(K, U, ie) {
                var de = "", ce = (typeof K == "string" && (K = JSON.parse(K)), typeof U == "string" && (U = JSON.parse(U)), Array.isArray(K) && 0 < K.length), be = !Array.isArray(U[0]);
                if (ce && N) {
                  for (var fe = 0; fe < K.length; fe++) 0 < fe && (de += L), de += k(K[fe], fe);
                  0 < U.length && (de += re);
                }
                for (var _e = 0; _e < U.length; _e++) {
                  var ke = (ce ? K : U[_e]).length, pe = false, Me = ce ? Object.keys(U[_e]).length === 0 : U[_e].length === 0;
                  if (ie && !ce && (pe = ie === "greedy" ? U[_e].join("").trim() === "" : U[_e].length === 1 && U[_e][0].length === 0), ie === "greedy" && ce) {
                    for (var Ze = [], Le = 0; Le < ke; Le++) {
                      var je = be ? K[Le] : Le;
                      Ze.push(U[_e][je]);
                    }
                    pe = Ze.join("").trim() === "";
                  }
                  if (!pe) {
                    for (var Ae = 0; Ae < ke; Ae++) {
                      0 < Ae && !Me && (de += L);
                      var nt = ce && be ? K[Ae] : Ae;
                      de += k(U[_e][nt], Ae);
                    }
                    _e < U.length - 1 && (!ie || 0 < ke && !Me) && (de += re);
                  }
                }
                return de;
              }
              function k(K, U) {
                var ie, de;
                return K == null ? "" : K.constructor === Date ? JSON.stringify(K).slice(1, 25) : (de = false, $ && typeof K == "string" && $.test(K) && (K = "'" + K, de = true), ie = K.toString().replace(ne, oe), (de = de || S === true || typeof S == "function" && S(K, U) || Array.isArray(S) && S[U] || ((ce, be) => {
                  for (var fe = 0; fe < be.length; fe++) if (-1 < ce.indexOf(be[fe])) return true;
                  return false;
                })(ie, g.BAD_DELIMITERS) || -1 < ie.indexOf(L) || ie.charAt(0) === " " || ie.charAt(ie.length - 1) === " ") ? ue + ie + ue : ie);
              }
            }, g.RECORD_SEP = "", g.UNIT_SEP = "", g.BYTE_ORDER_MARK = "\uFEFF", g.BAD_DELIMITERS = [
              "\r",
              `
`,
              '"',
              g.BYTE_ORDER_MARK
            ], g.WORKERS_SUPPORTED = !c && !!i.Worker, g.NODE_STREAM_INPUT = 1, g.LocalChunkSize = 10485760, g.RemoteChunkSize = 5242880, g.DefaultDelimiter = ",", g.Parser = te, g.ParserHandle = H, g.NetworkStreamer = _, g.FileStreamer = E, g.StringStreamer = R, g.ReadableStreamStreamer = z, i.jQuery && ((u = i.jQuery).fn.parse = function(y) {
              var w = y.config || {}, S = [];
              return this.each(function(re) {
                if (!(u(this).prop("tagName").toUpperCase() === "INPUT" && u(this).attr("type").toLowerCase() === "file" && i.FileReader) || !this.files || this.files.length === 0) return true;
                for (var ue = 0; ue < this.files.length; ue++) S.push({
                  file: this.files[ue],
                  inputElem: this,
                  instanceConfig: u.extend({}, w)
                });
              }), N(), this;
              function N() {
                if (S.length === 0) T(y.complete) && y.complete();
                else {
                  var re, ue, oe, D, X = S[0];
                  if (T(y.before)) {
                    var $ = y.before(X.file, X.inputElem);
                    if (typeof $ == "object") {
                      if ($.action === "abort") return re = "AbortError", ue = X.file, oe = X.inputElem, D = $.reason, void (T(y.error) && y.error({
                        name: re
                      }, ue, oe, D));
                      if ($.action === "skip") return void L();
                      typeof $.config == "object" && (X.instanceConfig = u.extend(X.instanceConfig, $.config));
                    } else if ($ === "skip") return void L();
                  }
                  var ne = X.instanceConfig.complete;
                  X.instanceConfig.complete = function(x) {
                    T(ne) && ne(x, X.file, X.inputElem), L();
                  }, g.parse(X.file, X.instanceConfig);
                }
              }
              function L() {
                S.splice(0, 1), N();
              }
            }), d && (i.onmessage = function(y) {
              y = y.data, g.WORKER_ID === void 0 && y && (g.WORKER_ID = y.workerId), typeof y.input == "string" ? i.postMessage({
                workerId: g.WORKER_ID,
                results: g.parse(y.input, y.config),
                finished: true
              }) : (i.File && y.input instanceof File || y.input instanceof Object) && (y = g.parse(y.input, y.config)) && i.postMessage({
                workerId: g.WORKER_ID,
                results: y,
                finished: true
              });
            }), (_.prototype = Object.create(p.prototype)).constructor = _, (E.prototype = Object.create(p.prototype)).constructor = E, (R.prototype = Object.create(R.prototype)).constructor = R, (z.prototype = Object.create(p.prototype)).constructor = z, g;
          });
        }(So)), So.exports;
      }
      var cw = sw();
      const fw = Lc(cw);
      var go = {
        exports: {}
      }, rm;
      function dw() {
        if (rm) return go.exports;
        rm = 1;
        var l = typeof Reflect == "object" ? Reflect : null, a = l && typeof l.apply == "function" ? l.apply : function(B, Z, T) {
          return Function.prototype.apply.call(B, Z, T);
        }, o;
        l && typeof l.ownKeys == "function" ? o = l.ownKeys : Object.getOwnPropertySymbols ? o = function(B) {
          return Object.getOwnPropertyNames(B).concat(Object.getOwnPropertySymbols(B));
        } : o = function(B) {
          return Object.getOwnPropertyNames(B);
        };
        function i(G) {
          console && console.warn && console.warn(G);
        }
        var u = Number.isNaN || function(B) {
          return B !== B;
        };
        function c() {
          c.init.call(this);
        }
        go.exports = c, go.exports.once = te, c.EventEmitter = c, c.prototype._events = void 0, c.prototype._eventsCount = 0, c.prototype._maxListeners = void 0;
        var d = 10;
        function h(G) {
          if (typeof G != "function") throw new TypeError('The "listener" argument must be of type Function. Received type ' + typeof G);
        }
        Object.defineProperty(c, "defaultMaxListeners", {
          enumerable: true,
          get: function() {
            return d;
          },
          set: function(G) {
            if (typeof G != "number" || G < 0 || u(G)) throw new RangeError('The value of "defaultMaxListeners" is out of range. It must be a non-negative number. Received ' + G + ".");
            d = G;
          }
        }), c.init = function() {
          (this._events === void 0 || this._events === Object.getPrototypeOf(this)._events) && (this._events = /* @__PURE__ */ Object.create(null), this._eventsCount = 0), this._maxListeners = this._maxListeners || void 0;
        }, c.prototype.setMaxListeners = function(B) {
          if (typeof B != "number" || B < 0 || u(B)) throw new RangeError('The value of "n" is out of range. It must be a non-negative number. Received ' + B + ".");
          return this._maxListeners = B, this;
        };
        function m(G) {
          return G._maxListeners === void 0 ? c.defaultMaxListeners : G._maxListeners;
        }
        c.prototype.getMaxListeners = function() {
          return m(this);
        }, c.prototype.emit = function(B) {
          for (var Z = [], T = 1; T < arguments.length; T++) Z.push(arguments[T]);
          var y = B === "error", w = this._events;
          if (w !== void 0) y = y && w.error === void 0;
          else if (!y) return false;
          if (y) {
            var S;
            if (Z.length > 0 && (S = Z[0]), S instanceof Error) throw S;
            var N = new Error("Unhandled error." + (S ? " (" + S.message + ")" : ""));
            throw N.context = S, N;
          }
          var L = w[B];
          if (L === void 0) return false;
          if (typeof L == "function") a(L, this, Z);
          else for (var re = L.length, ue = z(L, re), T = 0; T < re; ++T) a(ue[T], this, Z);
          return true;
        };
        function g(G, B, Z, T) {
          var y, w, S;
          if (h(Z), w = G._events, w === void 0 ? (w = G._events = /* @__PURE__ */ Object.create(null), G._eventsCount = 0) : (w.newListener !== void 0 && (G.emit("newListener", B, Z.listener ? Z.listener : Z), w = G._events), S = w[B]), S === void 0) S = w[B] = Z, ++G._eventsCount;
          else if (typeof S == "function" ? S = w[B] = T ? [
            Z,
            S
          ] : [
            S,
            Z
          ] : T ? S.unshift(Z) : S.push(Z), y = m(G), y > 0 && S.length > y && !S.warned) {
            S.warned = true;
            var N = new Error("Possible EventEmitter memory leak detected. " + S.length + " " + String(B) + " listeners added. Use emitter.setMaxListeners() to increase limit");
            N.name = "MaxListenersExceededWarning", N.emitter = G, N.type = B, N.count = S.length, i(N);
          }
          return G;
        }
        c.prototype.addListener = function(B, Z) {
          return g(this, B, Z, false);
        }, c.prototype.on = c.prototype.addListener, c.prototype.prependListener = function(B, Z) {
          return g(this, B, Z, true);
        };
        function p() {
          if (!this.fired) return this.target.removeListener(this.type, this.wrapFn), this.fired = true, arguments.length === 0 ? this.listener.call(this.target) : this.listener.apply(this.target, arguments);
        }
        function _(G, B, Z) {
          var T = {
            fired: false,
            wrapFn: void 0,
            target: G,
            type: B,
            listener: Z
          }, y = p.bind(T);
          return y.listener = Z, T.wrapFn = y, y;
        }
        c.prototype.once = function(B, Z) {
          return h(Z), this.on(B, _(this, B, Z)), this;
        }, c.prototype.prependOnceListener = function(B, Z) {
          return h(Z), this.prependListener(B, _(this, B, Z)), this;
        }, c.prototype.removeListener = function(B, Z) {
          var T, y, w, S, N;
          if (h(Z), y = this._events, y === void 0) return this;
          if (T = y[B], T === void 0) return this;
          if (T === Z || T.listener === Z) --this._eventsCount === 0 ? this._events = /* @__PURE__ */ Object.create(null) : (delete y[B], y.removeListener && this.emit("removeListener", B, T.listener || Z));
          else if (typeof T != "function") {
            for (w = -1, S = T.length - 1; S >= 0; S--) if (T[S] === Z || T[S].listener === Z) {
              N = T[S].listener, w = S;
              break;
            }
            if (w < 0) return this;
            w === 0 ? T.shift() : H(T, w), T.length === 1 && (y[B] = T[0]), y.removeListener !== void 0 && this.emit("removeListener", B, N || Z);
          }
          return this;
        }, c.prototype.off = c.prototype.removeListener, c.prototype.removeAllListeners = function(B) {
          var Z, T, y;
          if (T = this._events, T === void 0) return this;
          if (T.removeListener === void 0) return arguments.length === 0 ? (this._events = /* @__PURE__ */ Object.create(null), this._eventsCount = 0) : T[B] !== void 0 && (--this._eventsCount === 0 ? this._events = /* @__PURE__ */ Object.create(null) : delete T[B]), this;
          if (arguments.length === 0) {
            var w = Object.keys(T), S;
            for (y = 0; y < w.length; ++y) S = w[y], S !== "removeListener" && this.removeAllListeners(S);
            return this.removeAllListeners("removeListener"), this._events = /* @__PURE__ */ Object.create(null), this._eventsCount = 0, this;
          }
          if (Z = T[B], typeof Z == "function") this.removeListener(B, Z);
          else if (Z !== void 0) for (y = Z.length - 1; y >= 0; y--) this.removeListener(B, Z[y]);
          return this;
        };
        function E(G, B, Z) {
          var T = G._events;
          if (T === void 0) return [];
          var y = T[B];
          return y === void 0 ? [] : typeof y == "function" ? Z ? [
            y.listener || y
          ] : [
            y
          ] : Z ? Y(y) : z(y, y.length);
        }
        c.prototype.listeners = function(B) {
          return E(this, B, true);
        }, c.prototype.rawListeners = function(B) {
          return E(this, B, false);
        }, c.listenerCount = function(G, B) {
          return typeof G.listenerCount == "function" ? G.listenerCount(B) : R.call(G, B);
        }, c.prototype.listenerCount = R;
        function R(G) {
          var B = this._events;
          if (B !== void 0) {
            var Z = B[G];
            if (typeof Z == "function") return 1;
            if (Z !== void 0) return Z.length;
          }
          return 0;
        }
        c.prototype.eventNames = function() {
          return this._eventsCount > 0 ? o(this._events) : [];
        };
        function z(G, B) {
          for (var Z = new Array(B), T = 0; T < B; ++T) Z[T] = G[T];
          return Z;
        }
        function H(G, B) {
          for (; B + 1 < G.length; B++) G[B] = G[B + 1];
          G.pop();
        }
        function Y(G) {
          for (var B = new Array(G.length), Z = 0; Z < B.length; ++Z) B[Z] = G[Z].listener || G[Z];
          return B;
        }
        function te(G, B) {
          return new Promise(function(Z, T) {
            function y(S) {
              G.removeListener(B, w), T(S);
            }
            function w() {
              typeof G.removeListener == "function" && G.removeListener("error", y), Z([].slice.call(arguments));
            }
            I(G, B, w, {
              once: true
            }), B !== "error" && J(G, y, {
              once: true
            });
          });
        }
        function J(G, B, Z) {
          typeof G.on == "function" && I(G, "error", B, Z);
        }
        function I(G, B, Z, T) {
          if (typeof G.on == "function") T.once ? G.once(B, Z) : G.on(B, Z);
          else if (typeof G.addEventListener == "function") G.addEventListener(B, function y(w) {
            T.once && G.removeEventListener(B, y), Z(w);
          });
          else throw new TypeError('The "emitter" argument must be of type EventEmitter. Received type ' + typeof G);
        }
        return go.exports;
      }
      var yv = dw();
      function hw() {
        const l = arguments[0];
        for (let a = 1, o = arguments.length; a < o; a++) if (arguments[a]) for (const i in arguments[a]) l[i] = arguments[a][i];
        return l;
      }
      let gt = hw;
      typeof Object.assign == "function" && (gt = Object.assign);
      function rn(l, a, o, i) {
        const u = l._nodes.get(a);
        let c = null;
        return u && (i === "mixed" ? c = u.out && u.out[o] || u.undirected && u.undirected[o] : i === "directed" ? c = u.out && u.out[o] : c = u.undirected && u.undirected[o]), c;
      }
      function Tt(l) {
        return typeof l == "object" && l !== null;
      }
      function bv(l) {
        let a;
        for (a in l) return false;
        return true;
      }
      function an(l, a, o) {
        Object.defineProperty(l, a, {
          enumerable: false,
          configurable: false,
          writable: true,
          value: o
        });
      }
      function hn(l, a, o) {
        const i = {
          enumerable: true,
          configurable: true
        };
        typeof o == "function" ? i.get = o : (i.value = o, i.writable = false), Object.defineProperty(l, a, i);
      }
      function lm(l) {
        return !(!Tt(l) || l.attributes && !Array.isArray(l.attributes));
      }
      function gw() {
        let l = Math.floor(Math.random() * 256) & 255;
        return () => l++;
      }
      function Yn() {
        const l = arguments;
        let a = null, o = -1;
        return {
          [Symbol.iterator]() {
            return this;
          },
          next() {
            let i = null;
            do {
              if (a === null) {
                if (o++, o >= l.length) return {
                  done: true
                };
                a = l[o][Symbol.iterator]();
              }
              if (i = a.next(), i.done) {
                a = null;
                continue;
              }
              break;
            } while (true);
            return i;
          }
        };
      }
      function Ga() {
        return {
          [Symbol.iterator]() {
            return this;
          },
          next() {
            return {
              done: true
            };
          }
        };
      }
      class Yc extends Error {
        constructor(a) {
          super(), this.name = "GraphError", this.message = a;
        }
      }
      class se extends Yc {
        constructor(a) {
          super(a), this.name = "InvalidArgumentsGraphError", typeof Error.captureStackTrace == "function" && Error.captureStackTrace(this, se.prototype.constructor);
        }
      }
      class le extends Yc {
        constructor(a) {
          super(a), this.name = "NotFoundGraphError", typeof Error.captureStackTrace == "function" && Error.captureStackTrace(this, le.prototype.constructor);
        }
      }
      class Ee extends Yc {
        constructor(a) {
          super(a), this.name = "UsageGraphError", typeof Error.captureStackTrace == "function" && Error.captureStackTrace(this, Ee.prototype.constructor);
        }
      }
      function _v(l, a) {
        this.key = l, this.attributes = a, this.clear();
      }
      _v.prototype.clear = function() {
        this.inDegree = 0, this.outDegree = 0, this.undirectedDegree = 0, this.undirectedLoops = 0, this.directedLoops = 0, this.in = {}, this.out = {}, this.undirected = {};
      };
      function wv(l, a) {
        this.key = l, this.attributes = a, this.clear();
      }
      wv.prototype.clear = function() {
        this.inDegree = 0, this.outDegree = 0, this.directedLoops = 0, this.in = {}, this.out = {};
      };
      function Ev(l, a) {
        this.key = l, this.attributes = a, this.clear();
      }
      Ev.prototype.clear = function() {
        this.undirectedDegree = 0, this.undirectedLoops = 0, this.undirected = {};
      };
      function Ua(l, a, o, i, u) {
        this.key = a, this.attributes = u, this.undirected = l, this.source = o, this.target = i;
      }
      Ua.prototype.attach = function() {
        let l = "out", a = "in";
        this.undirected && (l = a = "undirected");
        const o = this.source.key, i = this.target.key;
        this.source[l][i] = this, !(this.undirected && o === i) && (this.target[a][o] = this);
      };
      Ua.prototype.attachMulti = function() {
        let l = "out", a = "in";
        const o = this.source.key, i = this.target.key;
        this.undirected && (l = a = "undirected");
        const u = this.source[l], c = u[i];
        if (typeof c > "u") {
          u[i] = this, this.undirected && o === i || (this.target[a][o] = this);
          return;
        }
        c.previous = this, this.next = c, u[i] = this, this.target[a][o] = this;
      };
      Ua.prototype.detach = function() {
        const l = this.source.key, a = this.target.key;
        let o = "out", i = "in";
        this.undirected && (o = i = "undirected"), delete this.source[o][a], delete this.target[i][l];
      };
      Ua.prototype.detachMulti = function() {
        const l = this.source.key, a = this.target.key;
        let o = "out", i = "in";
        this.undirected && (o = i = "undirected"), this.previous === void 0 ? this.next === void 0 ? (delete this.source[o][a], delete this.target[i][l]) : (this.next.previous = void 0, this.source[o][a] = this.next, this.target[i][l] = this.next) : (this.previous.next = this.next, this.next !== void 0 && (this.next.previous = this.previous));
      };
      const Sv = 0, xv = 1, mw = 2, Tv = 3;
      function $n(l, a, o, i, u, c, d) {
        let h, m, g, p;
        if (i = "" + i, o === Sv) {
          if (h = l._nodes.get(i), !h) throw new le(`Graph.${a}: could not find the "${i}" node in the graph.`);
          g = u, p = c;
        } else if (o === Tv) {
          if (u = "" + u, m = l._edges.get(u), !m) throw new le(`Graph.${a}: could not find the "${u}" edge in the graph.`);
          const _ = m.source.key, E = m.target.key;
          if (i === _) h = m.target;
          else if (i === E) h = m.source;
          else throw new le(`Graph.${a}: the "${i}" node is not attached to the "${u}" edge (${_}, ${E}).`);
          g = c, p = d;
        } else {
          if (m = l._edges.get(i), !m) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          o === xv ? h = m.source : h = m.target, g = u, p = c;
        }
        return [
          h,
          g,
          p
        ];
      }
      function vw(l, a, o) {
        l.prototype[a] = function(i, u, c) {
          const [d, h] = $n(this, a, o, i, u, c);
          return d.attributes[h];
        };
      }
      function pw(l, a, o) {
        l.prototype[a] = function(i, u) {
          const [c] = $n(this, a, o, i, u);
          return c.attributes;
        };
      }
      function yw(l, a, o) {
        l.prototype[a] = function(i, u, c) {
          const [d, h] = $n(this, a, o, i, u, c);
          return d.attributes.hasOwnProperty(h);
        };
      }
      function bw(l, a, o) {
        l.prototype[a] = function(i, u, c, d) {
          const [h, m, g] = $n(this, a, o, i, u, c, d);
          return h.attributes[m] = g, this.emit("nodeAttributesUpdated", {
            key: h.key,
            type: "set",
            attributes: h.attributes,
            name: m
          }), this;
        };
      }
      function _w(l, a, o) {
        l.prototype[a] = function(i, u, c, d) {
          const [h, m, g] = $n(this, a, o, i, u, c, d);
          if (typeof g != "function") throw new se(`Graph.${a}: updater should be a function.`);
          const p = h.attributes, _ = g(p[m]);
          return p[m] = _, this.emit("nodeAttributesUpdated", {
            key: h.key,
            type: "set",
            attributes: h.attributes,
            name: m
          }), this;
        };
      }
      function ww(l, a, o) {
        l.prototype[a] = function(i, u, c) {
          const [d, h] = $n(this, a, o, i, u, c);
          return delete d.attributes[h], this.emit("nodeAttributesUpdated", {
            key: d.key,
            type: "remove",
            attributes: d.attributes,
            name: h
          }), this;
        };
      }
      function Ew(l, a, o) {
        l.prototype[a] = function(i, u, c) {
          const [d, h] = $n(this, a, o, i, u, c);
          if (!Tt(h)) throw new se(`Graph.${a}: provided attributes are not a plain object.`);
          return d.attributes = h, this.emit("nodeAttributesUpdated", {
            key: d.key,
            type: "replace",
            attributes: d.attributes
          }), this;
        };
      }
      function Sw(l, a, o) {
        l.prototype[a] = function(i, u, c) {
          const [d, h] = $n(this, a, o, i, u, c);
          if (!Tt(h)) throw new se(`Graph.${a}: provided attributes are not a plain object.`);
          return gt(d.attributes, h), this.emit("nodeAttributesUpdated", {
            key: d.key,
            type: "merge",
            attributes: d.attributes,
            data: h
          }), this;
        };
      }
      function xw(l, a, o) {
        l.prototype[a] = function(i, u, c) {
          const [d, h] = $n(this, a, o, i, u, c);
          if (typeof h != "function") throw new se(`Graph.${a}: provided updater is not a function.`);
          return d.attributes = h(d.attributes), this.emit("nodeAttributesUpdated", {
            key: d.key,
            type: "update",
            attributes: d.attributes
          }), this;
        };
      }
      const Tw = [
        {
          name: (l) => `get${l}Attribute`,
          attacher: vw
        },
        {
          name: (l) => `get${l}Attributes`,
          attacher: pw
        },
        {
          name: (l) => `has${l}Attribute`,
          attacher: yw
        },
        {
          name: (l) => `set${l}Attribute`,
          attacher: bw
        },
        {
          name: (l) => `update${l}Attribute`,
          attacher: _w
        },
        {
          name: (l) => `remove${l}Attribute`,
          attacher: ww
        },
        {
          name: (l) => `replace${l}Attributes`,
          attacher: Ew
        },
        {
          name: (l) => `merge${l}Attributes`,
          attacher: Sw
        },
        {
          name: (l) => `update${l}Attributes`,
          attacher: xw
        }
      ];
      function Aw(l) {
        Tw.forEach(function({ name: a, attacher: o }) {
          o(l, a("Node"), Sv), o(l, a("Source"), xv), o(l, a("Target"), mw), o(l, a("Opposite"), Tv);
        });
      }
      function Rw(l, a, o) {
        l.prototype[a] = function(i, u) {
          let c;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 2) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const d = "" + i, h = "" + u;
            if (u = arguments[2], c = rn(this, d, h, o), !c) throw new le(`Graph.${a}: could not find an edge for the given path ("${d}" - "${h}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, c = this._edges.get(i), !c) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          return c.attributes[u];
        };
      }
      function Cw(l, a, o) {
        l.prototype[a] = function(i) {
          let u;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 1) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const c = "" + i, d = "" + arguments[1];
            if (u = rn(this, c, d, o), !u) throw new le(`Graph.${a}: could not find an edge for the given path ("${c}" - "${d}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, u = this._edges.get(i), !u) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          return u.attributes;
        };
      }
      function Dw(l, a, o) {
        l.prototype[a] = function(i, u) {
          let c;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 2) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const d = "" + i, h = "" + u;
            if (u = arguments[2], c = rn(this, d, h, o), !c) throw new le(`Graph.${a}: could not find an edge for the given path ("${d}" - "${h}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, c = this._edges.get(i), !c) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          return c.attributes.hasOwnProperty(u);
        };
      }
      function Nw(l, a, o) {
        l.prototype[a] = function(i, u, c) {
          let d;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 3) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const h = "" + i, m = "" + u;
            if (u = arguments[2], c = arguments[3], d = rn(this, h, m, o), !d) throw new le(`Graph.${a}: could not find an edge for the given path ("${h}" - "${m}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, d = this._edges.get(i), !d) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          return d.attributes[u] = c, this.emit("edgeAttributesUpdated", {
            key: d.key,
            type: "set",
            attributes: d.attributes,
            name: u
          }), this;
        };
      }
      function Ow(l, a, o) {
        l.prototype[a] = function(i, u, c) {
          let d;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 3) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const h = "" + i, m = "" + u;
            if (u = arguments[2], c = arguments[3], d = rn(this, h, m, o), !d) throw new le(`Graph.${a}: could not find an edge for the given path ("${h}" - "${m}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, d = this._edges.get(i), !d) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          if (typeof c != "function") throw new se(`Graph.${a}: updater should be a function.`);
          return d.attributes[u] = c(d.attributes[u]), this.emit("edgeAttributesUpdated", {
            key: d.key,
            type: "set",
            attributes: d.attributes,
            name: u
          }), this;
        };
      }
      function zw(l, a, o) {
        l.prototype[a] = function(i, u) {
          let c;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 2) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const d = "" + i, h = "" + u;
            if (u = arguments[2], c = rn(this, d, h, o), !c) throw new le(`Graph.${a}: could not find an edge for the given path ("${d}" - "${h}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, c = this._edges.get(i), !c) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          return delete c.attributes[u], this.emit("edgeAttributesUpdated", {
            key: c.key,
            type: "remove",
            attributes: c.attributes,
            name: u
          }), this;
        };
      }
      function kw(l, a, o) {
        l.prototype[a] = function(i, u) {
          let c;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 2) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const d = "" + i, h = "" + u;
            if (u = arguments[2], c = rn(this, d, h, o), !c) throw new le(`Graph.${a}: could not find an edge for the given path ("${d}" - "${h}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, c = this._edges.get(i), !c) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          if (!Tt(u)) throw new se(`Graph.${a}: provided attributes are not a plain object.`);
          return c.attributes = u, this.emit("edgeAttributesUpdated", {
            key: c.key,
            type: "replace",
            attributes: c.attributes
          }), this;
        };
      }
      function Mw(l, a, o) {
        l.prototype[a] = function(i, u) {
          let c;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 2) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const d = "" + i, h = "" + u;
            if (u = arguments[2], c = rn(this, d, h, o), !c) throw new le(`Graph.${a}: could not find an edge for the given path ("${d}" - "${h}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, c = this._edges.get(i), !c) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          if (!Tt(u)) throw new se(`Graph.${a}: provided attributes are not a plain object.`);
          return gt(c.attributes, u), this.emit("edgeAttributesUpdated", {
            key: c.key,
            type: "merge",
            attributes: c.attributes,
            data: u
          }), this;
        };
      }
      function Lw(l, a, o) {
        l.prototype[a] = function(i, u) {
          let c;
          if (this.type !== "mixed" && o !== "mixed" && o !== this.type) throw new Ee(`Graph.${a}: cannot find this type of edges in your ${this.type} graph.`);
          if (arguments.length > 2) {
            if (this.multi) throw new Ee(`Graph.${a}: cannot use a {source,target} combo when asking about an edge's attributes in a MultiGraph since we cannot infer the one you want information about.`);
            const d = "" + i, h = "" + u;
            if (u = arguments[2], c = rn(this, d, h, o), !c) throw new le(`Graph.${a}: could not find an edge for the given path ("${d}" - "${h}").`);
          } else {
            if (o !== "mixed") throw new Ee(`Graph.${a}: calling this method with only a key (vs. a source and target) does not make sense since an edge with this key could have the other type.`);
            if (i = "" + i, c = this._edges.get(i), !c) throw new le(`Graph.${a}: could not find the "${i}" edge in the graph.`);
          }
          if (typeof u != "function") throw new se(`Graph.${a}: provided updater is not a function.`);
          return c.attributes = u(c.attributes), this.emit("edgeAttributesUpdated", {
            key: c.key,
            type: "update",
            attributes: c.attributes
          }), this;
        };
      }
      const Gw = [
        {
          name: (l) => `get${l}Attribute`,
          attacher: Rw
        },
        {
          name: (l) => `get${l}Attributes`,
          attacher: Cw
        },
        {
          name: (l) => `has${l}Attribute`,
          attacher: Dw
        },
        {
          name: (l) => `set${l}Attribute`,
          attacher: Nw
        },
        {
          name: (l) => `update${l}Attribute`,
          attacher: Ow
        },
        {
          name: (l) => `remove${l}Attribute`,
          attacher: zw
        },
        {
          name: (l) => `replace${l}Attributes`,
          attacher: kw
        },
        {
          name: (l) => `merge${l}Attributes`,
          attacher: Mw
        },
        {
          name: (l) => `update${l}Attributes`,
          attacher: Lw
        }
      ];
      function Uw(l) {
        Gw.forEach(function({ name: a, attacher: o }) {
          o(l, a("Edge"), "mixed"), o(l, a("DirectedEdge"), "directed"), o(l, a("UndirectedEdge"), "undirected");
        });
      }
      const jw = [
        {
          name: "edges",
          type: "mixed"
        },
        {
          name: "inEdges",
          type: "directed",
          direction: "in"
        },
        {
          name: "outEdges",
          type: "directed",
          direction: "out"
        },
        {
          name: "inboundEdges",
          type: "mixed",
          direction: "in"
        },
        {
          name: "outboundEdges",
          type: "mixed",
          direction: "out"
        },
        {
          name: "directedEdges",
          type: "directed"
        },
        {
          name: "undirectedEdges",
          type: "undirected"
        }
      ];
      function Bw(l, a, o, i) {
        let u = false;
        for (const c in a) {
          if (c === i) continue;
          const d = a[c];
          if (u = o(d.key, d.attributes, d.source.key, d.target.key, d.source.attributes, d.target.attributes, d.undirected), l && u) return d.key;
        }
      }
      function Hw(l, a, o, i) {
        let u, c, d, h = false;
        for (const m in a) if (m !== i) {
          u = a[m];
          do {
            if (c = u.source, d = u.target, h = o(u.key, u.attributes, c.key, d.key, c.attributes, d.attributes, u.undirected), l && h) return u.key;
            u = u.next;
          } while (u !== void 0);
        }
      }
      function ic(l, a) {
        const o = Object.keys(l), i = o.length;
        let u, c = 0;
        return {
          [Symbol.iterator]() {
            return this;
          },
          next() {
            do
              if (u) u = u.next;
              else {
                if (c >= i) return {
                  done: true
                };
                const d = o[c++];
                if (d === a) {
                  u = void 0;
                  continue;
                }
                u = l[d];
              }
            while (!u);
            return {
              done: false,
              value: {
                edge: u.key,
                attributes: u.attributes,
                source: u.source.key,
                target: u.target.key,
                sourceAttributes: u.source.attributes,
                targetAttributes: u.target.attributes,
                undirected: u.undirected
              }
            };
          }
        };
      }
      function Fw(l, a, o, i) {
        const u = a[o];
        if (!u) return;
        const c = u.source, d = u.target;
        if (i(u.key, u.attributes, c.key, d.key, c.attributes, d.attributes, u.undirected) && l) return u.key;
      }
      function qw(l, a, o, i) {
        let u = a[o];
        if (!u) return;
        let c = false;
        do {
          if (c = i(u.key, u.attributes, u.source.key, u.target.key, u.source.attributes, u.target.attributes, u.undirected), l && c) return u.key;
          u = u.next;
        } while (u !== void 0);
      }
      function ac(l, a) {
        let o = l[a];
        if (o.next !== void 0) return {
          [Symbol.iterator]() {
            return this;
          },
          next() {
            if (!o) return {
              done: true
            };
            const u = {
              edge: o.key,
              attributes: o.attributes,
              source: o.source.key,
              target: o.target.key,
              sourceAttributes: o.source.attributes,
              targetAttributes: o.target.attributes,
              undirected: o.undirected
            };
            return o = o.next, {
              done: false,
              value: u
            };
          }
        };
        let i = false;
        return {
          [Symbol.iterator]() {
            return this;
          },
          next() {
            return i === true ? {
              done: true
            } : (i = true, {
              done: false,
              value: {
                edge: o.key,
                attributes: o.attributes,
                source: o.source.key,
                target: o.target.key,
                sourceAttributes: o.source.attributes,
                targetAttributes: o.target.attributes,
                undirected: o.undirected
              }
            });
          }
        };
      }
      function Vw(l, a) {
        if (l.size === 0) return [];
        if (a === "mixed" || a === l.type) return Array.from(l._edges.keys());
        const o = a === "undirected" ? l.undirectedSize : l.directedSize, i = new Array(o), u = a === "undirected", c = l._edges.values();
        let d = 0, h, m;
        for (; h = c.next(), h.done !== true; ) m = h.value, m.undirected === u && (i[d++] = m.key);
        return i;
      }
      function Av(l, a, o, i) {
        if (a.size === 0) return;
        const u = o !== "mixed" && o !== a.type, c = o === "undirected";
        let d, h, m = false;
        const g = a._edges.values();
        for (; d = g.next(), d.done !== true; ) {
          if (h = d.value, u && h.undirected !== c) continue;
          const { key: p, attributes: _, source: E, target: R } = h;
          if (m = i(p, _, E.key, R.key, E.attributes, R.attributes, h.undirected), l && m) return p;
        }
      }
      function Yw(l, a) {
        if (l.size === 0) return Ga();
        const o = a !== "mixed" && a !== l.type, i = a === "undirected", u = l._edges.values();
        return {
          [Symbol.iterator]() {
            return this;
          },
          next() {
            let c, d;
            for (; ; ) {
              if (c = u.next(), c.done) return c;
              if (d = c.value, !(o && d.undirected !== i)) break;
            }
            return {
              value: {
                edge: d.key,
                attributes: d.attributes,
                source: d.source.key,
                target: d.target.key,
                sourceAttributes: d.source.attributes,
                targetAttributes: d.target.attributes,
                undirected: d.undirected
              },
              done: false
            };
          }
        };
      }
      function $c(l, a, o, i, u, c) {
        const d = a ? Hw : Bw;
        let h;
        if (o !== "undirected" && (i !== "out" && (h = d(l, u.in, c), l && h) || i !== "in" && (h = d(l, u.out, c, i ? void 0 : u.key), l && h)) || o !== "directed" && (h = d(l, u.undirected, c), l && h)) return h;
      }
      function $w(l, a, o, i) {
        const u = [];
        return $c(false, l, a, o, i, function(c) {
          u.push(c);
        }), u;
      }
      function Xw(l, a, o) {
        let i = Ga();
        return l !== "undirected" && (a !== "out" && typeof o.in < "u" && (i = Yn(i, ic(o.in))), a !== "in" && typeof o.out < "u" && (i = Yn(i, ic(o.out, a ? void 0 : o.key)))), l !== "directed" && typeof o.undirected < "u" && (i = Yn(i, ic(o.undirected))), i;
      }
      function Xc(l, a, o, i, u, c, d) {
        const h = o ? qw : Fw;
        let m;
        if (a !== "undirected" && (typeof u.in < "u" && i !== "out" && (m = h(l, u.in, c, d), l && m) || typeof u.out < "u" && i !== "in" && (i || u.key !== c) && (m = h(l, u.out, c, d), l && m)) || a !== "directed" && typeof u.undirected < "u" && (m = h(l, u.undirected, c, d), l && m)) return m;
      }
      function Zw(l, a, o, i, u) {
        const c = [];
        return Xc(false, l, a, o, i, u, function(d) {
          c.push(d);
        }), c;
      }
      function Qw(l, a, o, i) {
        let u = Ga();
        return l !== "undirected" && (typeof o.in < "u" && a !== "out" && i in o.in && (u = Yn(u, ac(o.in, i))), typeof o.out < "u" && a !== "in" && i in o.out && (a || o.key !== i) && (u = Yn(u, ac(o.out, i)))), l !== "directed" && typeof o.undirected < "u" && i in o.undirected && (u = Yn(u, ac(o.undirected, i))), u;
      }
      function Kw(l, a) {
        const { name: o, type: i, direction: u } = a;
        l.prototype[o] = function(c, d) {
          if (i !== "mixed" && this.type !== "mixed" && i !== this.type) return [];
          if (!arguments.length) return Vw(this, i);
          if (arguments.length === 1) {
            c = "" + c;
            const h = this._nodes.get(c);
            if (typeof h > "u") throw new le(`Graph.${o}: could not find the "${c}" node in the graph.`);
            return $w(this.multi, i === "mixed" ? this.type : i, u, h);
          }
          if (arguments.length === 2) {
            c = "" + c, d = "" + d;
            const h = this._nodes.get(c);
            if (!h) throw new le(`Graph.${o}:  could not find the "${c}" source node in the graph.`);
            if (!this._nodes.has(d)) throw new le(`Graph.${o}:  could not find the "${d}" target node in the graph.`);
            return Zw(i, this.multi, u, h, d);
          }
          throw new se(`Graph.${o}: too many arguments (expecting 0, 1 or 2 and got ${arguments.length}).`);
        };
      }
      function Pw(l, a) {
        const { name: o, type: i, direction: u } = a, c = "forEach" + o[0].toUpperCase() + o.slice(1, -1);
        l.prototype[c] = function(g, p, _) {
          if (!(i !== "mixed" && this.type !== "mixed" && i !== this.type)) {
            if (arguments.length === 1) return _ = g, Av(false, this, i, _);
            if (arguments.length === 2) {
              g = "" + g, _ = p;
              const E = this._nodes.get(g);
              if (typeof E > "u") throw new le(`Graph.${c}: could not find the "${g}" node in the graph.`);
              return $c(false, this.multi, i === "mixed" ? this.type : i, u, E, _);
            }
            if (arguments.length === 3) {
              g = "" + g, p = "" + p;
              const E = this._nodes.get(g);
              if (!E) throw new le(`Graph.${c}:  could not find the "${g}" source node in the graph.`);
              if (!this._nodes.has(p)) throw new le(`Graph.${c}:  could not find the "${p}" target node in the graph.`);
              return Xc(false, i, this.multi, u, E, p, _);
            }
            throw new se(`Graph.${c}: too many arguments (expecting 1, 2 or 3 and got ${arguments.length}).`);
          }
        };
        const d = "map" + o[0].toUpperCase() + o.slice(1);
        l.prototype[d] = function() {
          const g = Array.prototype.slice.call(arguments), p = g.pop();
          let _;
          if (g.length === 0) {
            let E = 0;
            i !== "directed" && (E += this.undirectedSize), i !== "undirected" && (E += this.directedSize), _ = new Array(E);
            let R = 0;
            g.push((z, H, Y, te, J, I, G) => {
              _[R++] = p(z, H, Y, te, J, I, G);
            });
          } else _ = [], g.push((E, R, z, H, Y, te, J) => {
            _.push(p(E, R, z, H, Y, te, J));
          });
          return this[c].apply(this, g), _;
        };
        const h = "filter" + o[0].toUpperCase() + o.slice(1);
        l.prototype[h] = function() {
          const g = Array.prototype.slice.call(arguments), p = g.pop(), _ = [];
          return g.push((E, R, z, H, Y, te, J) => {
            p(E, R, z, H, Y, te, J) && _.push(E);
          }), this[c].apply(this, g), _;
        };
        const m = "reduce" + o[0].toUpperCase() + o.slice(1);
        l.prototype[m] = function() {
          let g = Array.prototype.slice.call(arguments);
          if (g.length < 2 || g.length > 4) throw new se(`Graph.${m}: invalid number of arguments (expecting 2, 3 or 4 and got ${g.length}).`);
          if (typeof g[g.length - 1] == "function" && typeof g[g.length - 2] != "function") throw new se(`Graph.${m}: missing initial value. You must provide it because the callback takes more than one argument and we cannot infer the initial value from the first iteration, as you could with a simple array.`);
          let p, _;
          g.length === 2 ? (p = g[0], _ = g[1], g = []) : g.length === 3 ? (p = g[1], _ = g[2], g = [
            g[0]
          ]) : g.length === 4 && (p = g[2], _ = g[3], g = [
            g[0],
            g[1]
          ]);
          let E = _;
          return g.push((R, z, H, Y, te, J, I) => {
            E = p(E, R, z, H, Y, te, J, I);
          }), this[c].apply(this, g), E;
        };
      }
      function Iw(l, a) {
        const { name: o, type: i, direction: u } = a, c = "find" + o[0].toUpperCase() + o.slice(1, -1);
        l.prototype[c] = function(m, g, p) {
          if (i !== "mixed" && this.type !== "mixed" && i !== this.type) return false;
          if (arguments.length === 1) return p = m, Av(true, this, i, p);
          if (arguments.length === 2) {
            m = "" + m, p = g;
            const _ = this._nodes.get(m);
            if (typeof _ > "u") throw new le(`Graph.${c}: could not find the "${m}" node in the graph.`);
            return $c(true, this.multi, i === "mixed" ? this.type : i, u, _, p);
          }
          if (arguments.length === 3) {
            m = "" + m, g = "" + g;
            const _ = this._nodes.get(m);
            if (!_) throw new le(`Graph.${c}:  could not find the "${m}" source node in the graph.`);
            if (!this._nodes.has(g)) throw new le(`Graph.${c}:  could not find the "${g}" target node in the graph.`);
            return Xc(true, i, this.multi, u, _, g, p);
          }
          throw new se(`Graph.${c}: too many arguments (expecting 1, 2 or 3 and got ${arguments.length}).`);
        };
        const d = "some" + o[0].toUpperCase() + o.slice(1, -1);
        l.prototype[d] = function() {
          const m = Array.prototype.slice.call(arguments), g = m.pop();
          return m.push((_, E, R, z, H, Y, te) => g(_, E, R, z, H, Y, te)), !!this[c].apply(this, m);
        };
        const h = "every" + o[0].toUpperCase() + o.slice(1, -1);
        l.prototype[h] = function() {
          const m = Array.prototype.slice.call(arguments), g = m.pop();
          return m.push((_, E, R, z, H, Y, te) => !g(_, E, R, z, H, Y, te)), !this[c].apply(this, m);
        };
      }
      function Ww(l, a) {
        const { name: o, type: i, direction: u } = a, c = o.slice(0, -1) + "Entries";
        l.prototype[c] = function(d, h) {
          if (i !== "mixed" && this.type !== "mixed" && i !== this.type) return Ga();
          if (!arguments.length) return Yw(this, i);
          if (arguments.length === 1) {
            d = "" + d;
            const m = this._nodes.get(d);
            if (!m) throw new le(`Graph.${c}: could not find the "${d}" node in the graph.`);
            return Xw(i, u, m);
          }
          if (arguments.length === 2) {
            d = "" + d, h = "" + h;
            const m = this._nodes.get(d);
            if (!m) throw new le(`Graph.${c}:  could not find the "${d}" source node in the graph.`);
            if (!this._nodes.has(h)) throw new le(`Graph.${c}:  could not find the "${h}" target node in the graph.`);
            return Qw(i, u, m, h);
          }
          throw new se(`Graph.${c}: too many arguments (expecting 0, 1 or 2 and got ${arguments.length}).`);
        };
      }
      function Jw(l) {
        jw.forEach((a) => {
          Kw(l, a), Pw(l, a), Iw(l, a), Ww(l, a);
        });
      }
      const eE = [
        {
          name: "neighbors",
          type: "mixed"
        },
        {
          name: "inNeighbors",
          type: "directed",
          direction: "in"
        },
        {
          name: "outNeighbors",
          type: "directed",
          direction: "out"
        },
        {
          name: "inboundNeighbors",
          type: "mixed",
          direction: "in"
        },
        {
          name: "outboundNeighbors",
          type: "mixed",
          direction: "out"
        },
        {
          name: "directedNeighbors",
          type: "directed"
        },
        {
          name: "undirectedNeighbors",
          type: "undirected"
        }
      ];
      function No() {
        this.A = null, this.B = null;
      }
      No.prototype.wrap = function(l) {
        this.A === null ? this.A = l : this.B === null && (this.B = l);
      };
      No.prototype.has = function(l) {
        return this.A !== null && l in this.A || this.B !== null && l in this.B;
      };
      function jr(l, a, o, i, u) {
        for (const c in i) {
          const d = i[c], h = d.source, m = d.target, g = h === o ? m : h;
          if (a && a.has(g.key)) continue;
          const p = u(g.key, g.attributes);
          if (l && p) return g.key;
        }
      }
      function Zc(l, a, o, i, u) {
        if (a !== "mixed") {
          if (a === "undirected") return jr(l, null, i, i.undirected, u);
          if (typeof o == "string") return jr(l, null, i, i[o], u);
        }
        const c = new No();
        let d;
        if (a !== "undirected") {
          if (o !== "out") {
            if (d = jr(l, null, i, i.in, u), l && d) return d;
            c.wrap(i.in);
          }
          if (o !== "in") {
            if (d = jr(l, c, i, i.out, u), l && d) return d;
            c.wrap(i.out);
          }
        }
        if (a !== "directed" && (d = jr(l, c, i, i.undirected, u), l && d)) return d;
      }
      function tE(l, a, o) {
        if (l !== "mixed") {
          if (l === "undirected") return Object.keys(o.undirected);
          if (typeof a == "string") return Object.keys(o[a]);
        }
        const i = [];
        return Zc(false, l, a, o, function(u) {
          i.push(u);
        }), i;
      }
      function Br(l, a, o) {
        const i = Object.keys(o), u = i.length;
        let c = 0;
        return {
          [Symbol.iterator]() {
            return this;
          },
          next() {
            let d = null;
            do {
              if (c >= u) return l && l.wrap(o), {
                done: true
              };
              const h = o[i[c++]], m = h.source, g = h.target;
              if (d = m === a ? g : m, l && l.has(d.key)) {
                d = null;
                continue;
              }
            } while (d === null);
            return {
              done: false,
              value: {
                neighbor: d.key,
                attributes: d.attributes
              }
            };
          }
        };
      }
      function nE(l, a, o) {
        if (l !== "mixed") {
          if (l === "undirected") return Br(null, o, o.undirected);
          if (typeof a == "string") return Br(null, o, o[a]);
        }
        let i = Ga();
        const u = new No();
        return l !== "undirected" && (a !== "out" && (i = Yn(i, Br(u, o, o.in))), a !== "in" && (i = Yn(i, Br(u, o, o.out)))), l !== "directed" && (i = Yn(i, Br(u, o, o.undirected))), i;
      }
      function iE(l, a) {
        const { name: o, type: i, direction: u } = a;
        l.prototype[o] = function(c) {
          if (i !== "mixed" && this.type !== "mixed" && i !== this.type) return [];
          c = "" + c;
          const d = this._nodes.get(c);
          if (typeof d > "u") throw new le(`Graph.${o}: could not find the "${c}" node in the graph.`);
          return tE(i === "mixed" ? this.type : i, u, d);
        };
      }
      function aE(l, a) {
        const { name: o, type: i, direction: u } = a, c = "forEach" + o[0].toUpperCase() + o.slice(1, -1);
        l.prototype[c] = function(g, p) {
          if (i !== "mixed" && this.type !== "mixed" && i !== this.type) return;
          g = "" + g;
          const _ = this._nodes.get(g);
          if (typeof _ > "u") throw new le(`Graph.${c}: could not find the "${g}" node in the graph.`);
          Zc(false, i === "mixed" ? this.type : i, u, _, p);
        };
        const d = "map" + o[0].toUpperCase() + o.slice(1);
        l.prototype[d] = function(g, p) {
          const _ = [];
          return this[c](g, (E, R) => {
            _.push(p(E, R));
          }), _;
        };
        const h = "filter" + o[0].toUpperCase() + o.slice(1);
        l.prototype[h] = function(g, p) {
          const _ = [];
          return this[c](g, (E, R) => {
            p(E, R) && _.push(E);
          }), _;
        };
        const m = "reduce" + o[0].toUpperCase() + o.slice(1);
        l.prototype[m] = function(g, p, _) {
          if (arguments.length < 3) throw new se(`Graph.${m}: missing initial value. You must provide it because the callback takes more than one argument and we cannot infer the initial value from the first iteration, as you could with a simple array.`);
          let E = _;
          return this[c](g, (R, z) => {
            E = p(E, R, z);
          }), E;
        };
      }
      function rE(l, a) {
        const { name: o, type: i, direction: u } = a, c = o[0].toUpperCase() + o.slice(1, -1), d = "find" + c;
        l.prototype[d] = function(g, p) {
          if (i !== "mixed" && this.type !== "mixed" && i !== this.type) return;
          g = "" + g;
          const _ = this._nodes.get(g);
          if (typeof _ > "u") throw new le(`Graph.${d}: could not find the "${g}" node in the graph.`);
          return Zc(true, i === "mixed" ? this.type : i, u, _, p);
        };
        const h = "some" + c;
        l.prototype[h] = function(g, p) {
          return !!this[d](g, p);
        };
        const m = "every" + c;
        l.prototype[m] = function(g, p) {
          return !this[d](g, (E, R) => !p(E, R));
        };
      }
      function lE(l, a) {
        const { name: o, type: i, direction: u } = a, c = o.slice(0, -1) + "Entries";
        l.prototype[c] = function(d) {
          if (i !== "mixed" && this.type !== "mixed" && i !== this.type) return Ga();
          d = "" + d;
          const h = this._nodes.get(d);
          if (typeof h > "u") throw new le(`Graph.${c}: could not find the "${d}" node in the graph.`);
          return nE(i === "mixed" ? this.type : i, u, h);
        };
      }
      function oE(l) {
        eE.forEach((a) => {
          iE(l, a), aE(l, a), rE(l, a), lE(l, a);
        });
      }
      function mo(l, a, o, i, u) {
        const c = i._nodes.values(), d = i.type;
        let h, m, g, p, _, E;
        for (; h = c.next(), h.done !== true; ) {
          let R = false;
          if (m = h.value, d !== "undirected") {
            p = m.out;
            for (g in p) {
              _ = p[g];
              do
                E = _.target, R = true, u(m.key, E.key, m.attributes, E.attributes, _.key, _.attributes, _.undirected), _ = _.next;
              while (_);
            }
          }
          if (d !== "directed") {
            p = m.undirected;
            for (g in p) if (!(a && m.key > g)) {
              _ = p[g];
              do
                E = _.target, E.key !== g && (E = _.source), R = true, u(m.key, E.key, m.attributes, E.attributes, _.key, _.attributes, _.undirected), _ = _.next;
              while (_);
            }
          }
          o && !R && u(m.key, null, m.attributes, null, null, null, null);
        }
      }
      function uE(l, a) {
        const o = {
          key: l
        };
        return bv(a.attributes) || (o.attributes = gt({}, a.attributes)), o;
      }
      function sE(l, a, o) {
        const i = {
          key: a,
          source: o.source.key,
          target: o.target.key
        };
        return bv(o.attributes) || (i.attributes = gt({}, o.attributes)), l === "mixed" && o.undirected && (i.undirected = true), i;
      }
      function cE(l) {
        if (!Tt(l)) throw new se('Graph.import: invalid serialized node. A serialized node should be a plain object with at least a "key" property.');
        if (!("key" in l)) throw new se("Graph.import: serialized node is missing its key.");
        if ("attributes" in l && (!Tt(l.attributes) || l.attributes === null)) throw new se("Graph.import: invalid attributes. Attributes should be a plain object, null or omitted.");
      }
      function fE(l) {
        if (!Tt(l)) throw new se('Graph.import: invalid serialized edge. A serialized edge should be a plain object with at least a "source" & "target" property.');
        if (!("source" in l)) throw new se("Graph.import: serialized edge is missing its source.");
        if (!("target" in l)) throw new se("Graph.import: serialized edge is missing its target.");
        if ("attributes" in l && (!Tt(l.attributes) || l.attributes === null)) throw new se("Graph.import: invalid attributes. Attributes should be a plain object, null or omitted.");
        if ("undirected" in l && typeof l.undirected != "boolean") throw new se("Graph.import: invalid undirectedness information. Undirected should be boolean or omitted.");
      }
      const dE = gw(), hE = /* @__PURE__ */ new Set([
        "directed",
        "undirected",
        "mixed"
      ]), om = /* @__PURE__ */ new Set([
        "domain",
        "_events",
        "_eventsCount",
        "_maxListeners"
      ]), gE = [
        {
          name: (l) => `${l}Edge`,
          generateKey: true
        },
        {
          name: (l) => `${l}DirectedEdge`,
          generateKey: true,
          type: "directed"
        },
        {
          name: (l) => `${l}UndirectedEdge`,
          generateKey: true,
          type: "undirected"
        },
        {
          name: (l) => `${l}EdgeWithKey`
        },
        {
          name: (l) => `${l}DirectedEdgeWithKey`,
          type: "directed"
        },
        {
          name: (l) => `${l}UndirectedEdgeWithKey`,
          type: "undirected"
        }
      ], mE = {
        allowSelfLoops: true,
        multi: false,
        type: "mixed"
      };
      function vE(l, a, o) {
        if (o && !Tt(o)) throw new se(`Graph.addNode: invalid attributes. Expecting an object but got "${o}"`);
        if (a = "" + a, o = o || {}, l._nodes.has(a)) throw new Ee(`Graph.addNode: the "${a}" node already exist in the graph.`);
        const i = new l.NodeDataClass(a, o);
        return l._nodes.set(a, i), l.emit("nodeAdded", {
          key: a,
          attributes: o
        }), i;
      }
      function um(l, a, o) {
        const i = new l.NodeDataClass(a, o);
        return l._nodes.set(a, i), l.emit("nodeAdded", {
          key: a,
          attributes: o
        }), i;
      }
      function Rv(l, a, o, i, u, c, d, h) {
        if (!i && l.type === "undirected") throw new Ee(`Graph.${a}: you cannot add a directed edge to an undirected graph. Use the #.addEdge or #.addUndirectedEdge instead.`);
        if (i && l.type === "directed") throw new Ee(`Graph.${a}: you cannot add an undirected edge to a directed graph. Use the #.addEdge or #.addDirectedEdge instead.`);
        if (h && !Tt(h)) throw new se(`Graph.${a}: invalid attributes. Expecting an object but got "${h}"`);
        if (c = "" + c, d = "" + d, h = h || {}, !l.allowSelfLoops && c === d) throw new Ee(`Graph.${a}: source & target are the same ("${c}"), thus creating a loop explicitly forbidden by this graph 'allowSelfLoops' option set to false.`);
        const m = l._nodes.get(c), g = l._nodes.get(d);
        if (!m) throw new le(`Graph.${a}: source node "${c}" not found.`);
        if (!g) throw new le(`Graph.${a}: target node "${d}" not found.`);
        const p = {
          key: null,
          undirected: i,
          source: c,
          target: d,
          attributes: h
        };
        if (o) u = l._edgeKeyGenerator();
        else if (u = "" + u, l._edges.has(u)) throw new Ee(`Graph.${a}: the "${u}" edge already exists in the graph.`);
        if (!l.multi && (i ? typeof m.undirected[d] < "u" : typeof m.out[d] < "u")) throw new Ee(`Graph.${a}: an edge linking "${c}" to "${d}" already exists. If you really want to add multiple edges linking those nodes, you should create a multi graph by using the 'multi' option.`);
        const _ = new Ua(i, u, m, g, h);
        l._edges.set(u, _);
        const E = c === d;
        return i ? (m.undirectedDegree++, g.undirectedDegree++, E && (m.undirectedLoops++, l._undirectedSelfLoopCount++)) : (m.outDegree++, g.inDegree++, E && (m.directedLoops++, l._directedSelfLoopCount++)), l.multi ? _.attachMulti() : _.attach(), i ? l._undirectedSize++ : l._directedSize++, p.key = u, l.emit("edgeAdded", p), u;
      }
      function pE(l, a, o, i, u, c, d, h, m) {
        if (!i && l.type === "undirected") throw new Ee(`Graph.${a}: you cannot merge/update a directed edge to an undirected graph. Use the #.mergeEdge/#.updateEdge or #.addUndirectedEdge instead.`);
        if (i && l.type === "directed") throw new Ee(`Graph.${a}: you cannot merge/update an undirected edge to a directed graph. Use the #.mergeEdge/#.updateEdge or #.addDirectedEdge instead.`);
        if (h) {
          if (m) {
            if (typeof h != "function") throw new se(`Graph.${a}: invalid updater function. Expecting a function but got "${h}"`);
          } else if (!Tt(h)) throw new se(`Graph.${a}: invalid attributes. Expecting an object but got "${h}"`);
        }
        c = "" + c, d = "" + d;
        let g;
        if (m && (g = h, h = void 0), !l.allowSelfLoops && c === d) throw new Ee(`Graph.${a}: source & target are the same ("${c}"), thus creating a loop explicitly forbidden by this graph 'allowSelfLoops' option set to false.`);
        let p = l._nodes.get(c), _ = l._nodes.get(d), E, R;
        if (!o && (E = l._edges.get(u), E)) {
          if ((E.source.key !== c || E.target.key !== d) && (!i || E.source.key !== d || E.target.key !== c)) throw new Ee(`Graph.${a}: inconsistency detected when attempting to merge the "${u}" edge with "${c}" source & "${d}" target vs. ("${E.source.key}", "${E.target.key}").`);
          R = E;
        }
        if (!R && !l.multi && p && (R = i ? p.undirected[d] : p.out[d]), R) {
          const J = [
            R.key,
            false,
            false,
            false
          ];
          if (m ? !g : !h) return J;
          if (m) {
            const I = R.attributes;
            R.attributes = g(I), l.emit("edgeAttributesUpdated", {
              type: "replace",
              key: R.key,
              attributes: R.attributes
            });
          } else gt(R.attributes, h), l.emit("edgeAttributesUpdated", {
            type: "merge",
            key: R.key,
            attributes: R.attributes,
            data: h
          });
          return J;
        }
        h = h || {}, m && g && (h = g(h));
        const z = {
          key: null,
          undirected: i,
          source: c,
          target: d,
          attributes: h
        };
        if (o) u = l._edgeKeyGenerator();
        else if (u = "" + u, l._edges.has(u)) throw new Ee(`Graph.${a}: the "${u}" edge already exists in the graph.`);
        let H = false, Y = false;
        p || (p = um(l, c, {}), H = true, c === d && (_ = p, Y = true)), _ || (_ = um(l, d, {}), Y = true), E = new Ua(i, u, p, _, h), l._edges.set(u, E);
        const te = c === d;
        return i ? (p.undirectedDegree++, _.undirectedDegree++, te && (p.undirectedLoops++, l._undirectedSelfLoopCount++)) : (p.outDegree++, _.inDegree++, te && (p.directedLoops++, l._directedSelfLoopCount++)), l.multi ? E.attachMulti() : E.attach(), i ? l._undirectedSize++ : l._directedSize++, z.key = u, l.emit("edgeAdded", z), [
          u,
          true,
          H,
          Y
        ];
      }
      function Ca(l, a) {
        l._edges.delete(a.key);
        const { source: o, target: i, attributes: u } = a, c = a.undirected, d = o === i;
        c ? (o.undirectedDegree--, i.undirectedDegree--, d && (o.undirectedLoops--, l._undirectedSelfLoopCount--)) : (o.outDegree--, i.inDegree--, d && (o.directedLoops--, l._directedSelfLoopCount--)), l.multi ? a.detachMulti() : a.detach(), c ? l._undirectedSize-- : l._directedSize--, l.emit("edgeDropped", {
          key: a.key,
          attributes: u,
          source: o.key,
          target: i.key,
          undirected: c
        });
      }
      let We = class Ac extends yv.EventEmitter {
        constructor(a) {
          if (super(), a = gt({}, mE, a), typeof a.multi != "boolean") throw new se(`Graph.constructor: invalid 'multi' option. Expecting a boolean but got "${a.multi}".`);
          if (!hE.has(a.type)) throw new se(`Graph.constructor: invalid 'type' option. Should be one of "mixed", "directed" or "undirected" but got "${a.type}".`);
          if (typeof a.allowSelfLoops != "boolean") throw new se(`Graph.constructor: invalid 'allowSelfLoops' option. Expecting a boolean but got "${a.allowSelfLoops}".`);
          const o = a.type === "mixed" ? _v : a.type === "directed" ? wv : Ev;
          an(this, "NodeDataClass", o);
          const i = "geid_" + dE() + "_";
          let u = 0;
          const c = () => {
            let d;
            do
              d = i + u++;
            while (this._edges.has(d));
            return d;
          };
          an(this, "_attributes", {}), an(this, "_nodes", /* @__PURE__ */ new Map()), an(this, "_edges", /* @__PURE__ */ new Map()), an(this, "_directedSize", 0), an(this, "_undirectedSize", 0), an(this, "_directedSelfLoopCount", 0), an(this, "_undirectedSelfLoopCount", 0), an(this, "_edgeKeyGenerator", c), an(this, "_options", a), om.forEach((d) => an(this, d, this[d])), hn(this, "order", () => this._nodes.size), hn(this, "size", () => this._edges.size), hn(this, "directedSize", () => this._directedSize), hn(this, "undirectedSize", () => this._undirectedSize), hn(this, "selfLoopCount", () => this._directedSelfLoopCount + this._undirectedSelfLoopCount), hn(this, "directedSelfLoopCount", () => this._directedSelfLoopCount), hn(this, "undirectedSelfLoopCount", () => this._undirectedSelfLoopCount), hn(this, "multi", this._options.multi), hn(this, "type", this._options.type), hn(this, "allowSelfLoops", this._options.allowSelfLoops), hn(this, "implementation", () => "graphology");
        }
        _resetInstanceCounters() {
          this._directedSize = 0, this._undirectedSize = 0, this._directedSelfLoopCount = 0, this._undirectedSelfLoopCount = 0;
        }
        hasNode(a) {
          return this._nodes.has("" + a);
        }
        hasDirectedEdge(a, o) {
          if (this.type === "undirected") return false;
          if (arguments.length === 1) {
            const i = "" + a, u = this._edges.get(i);
            return !!u && !u.undirected;
          } else if (arguments.length === 2) {
            a = "" + a, o = "" + o;
            const i = this._nodes.get(a);
            return i ? i.out.hasOwnProperty(o) : false;
          }
          throw new se(`Graph.hasDirectedEdge: invalid arity (${arguments.length}, instead of 1 or 2). You can either ask for an edge id or for the existence of an edge between a source & a target.`);
        }
        hasUndirectedEdge(a, o) {
          if (this.type === "directed") return false;
          if (arguments.length === 1) {
            const i = "" + a, u = this._edges.get(i);
            return !!u && u.undirected;
          } else if (arguments.length === 2) {
            a = "" + a, o = "" + o;
            const i = this._nodes.get(a);
            return i ? i.undirected.hasOwnProperty(o) : false;
          }
          throw new se(`Graph.hasDirectedEdge: invalid arity (${arguments.length}, instead of 1 or 2). You can either ask for an edge id or for the existence of an edge between a source & a target.`);
        }
        hasEdge(a, o) {
          if (arguments.length === 1) {
            const i = "" + a;
            return this._edges.has(i);
          } else if (arguments.length === 2) {
            a = "" + a, o = "" + o;
            const i = this._nodes.get(a);
            return i ? typeof i.out < "u" && i.out.hasOwnProperty(o) || typeof i.undirected < "u" && i.undirected.hasOwnProperty(o) : false;
          }
          throw new se(`Graph.hasEdge: invalid arity (${arguments.length}, instead of 1 or 2). You can either ask for an edge id or for the existence of an edge between a source & a target.`);
        }
        directedEdge(a, o) {
          if (this.type === "undirected") return;
          if (a = "" + a, o = "" + o, this.multi) throw new Ee("Graph.directedEdge: this method is irrelevant with multigraphs since there might be multiple edges between source & target. See #.directedEdges instead.");
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.directedEdge: could not find the "${a}" source node in the graph.`);
          if (!this._nodes.has(o)) throw new le(`Graph.directedEdge: could not find the "${o}" target node in the graph.`);
          const u = i.out && i.out[o] || void 0;
          if (u) return u.key;
        }
        undirectedEdge(a, o) {
          if (this.type === "directed") return;
          if (a = "" + a, o = "" + o, this.multi) throw new Ee("Graph.undirectedEdge: this method is irrelevant with multigraphs since there might be multiple edges between source & target. See #.undirectedEdges instead.");
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.undirectedEdge: could not find the "${a}" source node in the graph.`);
          if (!this._nodes.has(o)) throw new le(`Graph.undirectedEdge: could not find the "${o}" target node in the graph.`);
          const u = i.undirected && i.undirected[o] || void 0;
          if (u) return u.key;
        }
        edge(a, o) {
          if (this.multi) throw new Ee("Graph.edge: this method is irrelevant with multigraphs since there might be multiple edges between source & target. See #.edges instead.");
          a = "" + a, o = "" + o;
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.edge: could not find the "${a}" source node in the graph.`);
          if (!this._nodes.has(o)) throw new le(`Graph.edge: could not find the "${o}" target node in the graph.`);
          const u = i.out && i.out[o] || i.undirected && i.undirected[o] || void 0;
          if (u) return u.key;
        }
        areDirectedNeighbors(a, o) {
          a = "" + a, o = "" + o;
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.areDirectedNeighbors: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? false : o in i.in || o in i.out;
        }
        areOutNeighbors(a, o) {
          a = "" + a, o = "" + o;
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.areOutNeighbors: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? false : o in i.out;
        }
        areInNeighbors(a, o) {
          a = "" + a, o = "" + o;
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.areInNeighbors: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? false : o in i.in;
        }
        areUndirectedNeighbors(a, o) {
          a = "" + a, o = "" + o;
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.areUndirectedNeighbors: could not find the "${a}" node in the graph.`);
          return this.type === "directed" ? false : o in i.undirected;
        }
        areNeighbors(a, o) {
          a = "" + a, o = "" + o;
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.areNeighbors: could not find the "${a}" node in the graph.`);
          return this.type !== "undirected" && (o in i.in || o in i.out) || this.type !== "directed" && o in i.undirected;
        }
        areInboundNeighbors(a, o) {
          a = "" + a, o = "" + o;
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.areInboundNeighbors: could not find the "${a}" node in the graph.`);
          return this.type !== "undirected" && o in i.in || this.type !== "directed" && o in i.undirected;
        }
        areOutboundNeighbors(a, o) {
          a = "" + a, o = "" + o;
          const i = this._nodes.get(a);
          if (!i) throw new le(`Graph.areOutboundNeighbors: could not find the "${a}" node in the graph.`);
          return this.type !== "undirected" && o in i.out || this.type !== "directed" && o in i.undirected;
        }
        inDegree(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.inDegree: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? 0 : o.inDegree;
        }
        outDegree(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.outDegree: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? 0 : o.outDegree;
        }
        directedDegree(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.directedDegree: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? 0 : o.inDegree + o.outDegree;
        }
        undirectedDegree(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.undirectedDegree: could not find the "${a}" node in the graph.`);
          return this.type === "directed" ? 0 : o.undirectedDegree;
        }
        inboundDegree(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.inboundDegree: could not find the "${a}" node in the graph.`);
          let i = 0;
          return this.type !== "directed" && (i += o.undirectedDegree), this.type !== "undirected" && (i += o.inDegree), i;
        }
        outboundDegree(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.outboundDegree: could not find the "${a}" node in the graph.`);
          let i = 0;
          return this.type !== "directed" && (i += o.undirectedDegree), this.type !== "undirected" && (i += o.outDegree), i;
        }
        degree(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.degree: could not find the "${a}" node in the graph.`);
          let i = 0;
          return this.type !== "directed" && (i += o.undirectedDegree), this.type !== "undirected" && (i += o.inDegree + o.outDegree), i;
        }
        inDegreeWithoutSelfLoops(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.inDegreeWithoutSelfLoops: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? 0 : o.inDegree - o.directedLoops;
        }
        outDegreeWithoutSelfLoops(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.outDegreeWithoutSelfLoops: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? 0 : o.outDegree - o.directedLoops;
        }
        directedDegreeWithoutSelfLoops(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.directedDegreeWithoutSelfLoops: could not find the "${a}" node in the graph.`);
          return this.type === "undirected" ? 0 : o.inDegree + o.outDegree - o.directedLoops * 2;
        }
        undirectedDegreeWithoutSelfLoops(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.undirectedDegreeWithoutSelfLoops: could not find the "${a}" node in the graph.`);
          return this.type === "directed" ? 0 : o.undirectedDegree - o.undirectedLoops * 2;
        }
        inboundDegreeWithoutSelfLoops(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.inboundDegreeWithoutSelfLoops: could not find the "${a}" node in the graph.`);
          let i = 0, u = 0;
          return this.type !== "directed" && (i += o.undirectedDegree, u += o.undirectedLoops * 2), this.type !== "undirected" && (i += o.inDegree, u += o.directedLoops), i - u;
        }
        outboundDegreeWithoutSelfLoops(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.outboundDegreeWithoutSelfLoops: could not find the "${a}" node in the graph.`);
          let i = 0, u = 0;
          return this.type !== "directed" && (i += o.undirectedDegree, u += o.undirectedLoops * 2), this.type !== "undirected" && (i += o.outDegree, u += o.directedLoops), i - u;
        }
        degreeWithoutSelfLoops(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.degreeWithoutSelfLoops: could not find the "${a}" node in the graph.`);
          let i = 0, u = 0;
          return this.type !== "directed" && (i += o.undirectedDegree, u += o.undirectedLoops * 2), this.type !== "undirected" && (i += o.inDegree + o.outDegree, u += o.directedLoops * 2), i - u;
        }
        source(a) {
          a = "" + a;
          const o = this._edges.get(a);
          if (!o) throw new le(`Graph.source: could not find the "${a}" edge in the graph.`);
          return o.source.key;
        }
        target(a) {
          a = "" + a;
          const o = this._edges.get(a);
          if (!o) throw new le(`Graph.target: could not find the "${a}" edge in the graph.`);
          return o.target.key;
        }
        extremities(a) {
          a = "" + a;
          const o = this._edges.get(a);
          if (!o) throw new le(`Graph.extremities: could not find the "${a}" edge in the graph.`);
          return [
            o.source.key,
            o.target.key
          ];
        }
        opposite(a, o) {
          a = "" + a, o = "" + o;
          const i = this._edges.get(o);
          if (!i) throw new le(`Graph.opposite: could not find the "${o}" edge in the graph.`);
          const u = i.source.key, c = i.target.key;
          if (a === u) return c;
          if (a === c) return u;
          throw new le(`Graph.opposite: the "${a}" node is not attached to the "${o}" edge (${u}, ${c}).`);
        }
        hasExtremity(a, o) {
          a = "" + a, o = "" + o;
          const i = this._edges.get(a);
          if (!i) throw new le(`Graph.hasExtremity: could not find the "${a}" edge in the graph.`);
          return i.source.key === o || i.target.key === o;
        }
        isUndirected(a) {
          a = "" + a;
          const o = this._edges.get(a);
          if (!o) throw new le(`Graph.isUndirected: could not find the "${a}" edge in the graph.`);
          return o.undirected;
        }
        isDirected(a) {
          a = "" + a;
          const o = this._edges.get(a);
          if (!o) throw new le(`Graph.isDirected: could not find the "${a}" edge in the graph.`);
          return !o.undirected;
        }
        isSelfLoop(a) {
          a = "" + a;
          const o = this._edges.get(a);
          if (!o) throw new le(`Graph.isSelfLoop: could not find the "${a}" edge in the graph.`);
          return o.source === o.target;
        }
        addNode(a, o) {
          return vE(this, a, o).key;
        }
        mergeNode(a, o) {
          if (o && !Tt(o)) throw new se(`Graph.mergeNode: invalid attributes. Expecting an object but got "${o}"`);
          a = "" + a, o = o || {};
          let i = this._nodes.get(a);
          return i ? (o && (gt(i.attributes, o), this.emit("nodeAttributesUpdated", {
            type: "merge",
            key: a,
            attributes: i.attributes,
            data: o
          })), [
            a,
            false
          ]) : (i = new this.NodeDataClass(a, o), this._nodes.set(a, i), this.emit("nodeAdded", {
            key: a,
            attributes: o
          }), [
            a,
            true
          ]);
        }
        updateNode(a, o) {
          if (o && typeof o != "function") throw new se(`Graph.updateNode: invalid updater function. Expecting a function but got "${o}"`);
          a = "" + a;
          let i = this._nodes.get(a);
          if (i) {
            if (o) {
              const c = i.attributes;
              i.attributes = o(c), this.emit("nodeAttributesUpdated", {
                type: "replace",
                key: a,
                attributes: i.attributes
              });
            }
            return [
              a,
              false
            ];
          }
          const u = o ? o({}) : {};
          return i = new this.NodeDataClass(a, u), this._nodes.set(a, i), this.emit("nodeAdded", {
            key: a,
            attributes: u
          }), [
            a,
            true
          ];
        }
        dropNode(a) {
          a = "" + a;
          const o = this._nodes.get(a);
          if (!o) throw new le(`Graph.dropNode: could not find the "${a}" node in the graph.`);
          let i;
          if (this.type !== "undirected") {
            for (const u in o.out) {
              i = o.out[u];
              do
                Ca(this, i), i = i.next;
              while (i);
            }
            for (const u in o.in) {
              i = o.in[u];
              do
                Ca(this, i), i = i.next;
              while (i);
            }
          }
          if (this.type !== "directed") for (const u in o.undirected) {
            i = o.undirected[u];
            do
              Ca(this, i), i = i.next;
            while (i);
          }
          this._nodes.delete(a), this.emit("nodeDropped", {
            key: a,
            attributes: o.attributes
          });
        }
        dropEdge(a) {
          let o;
          if (arguments.length > 1) {
            const i = "" + arguments[0], u = "" + arguments[1];
            if (o = rn(this, i, u, this.type), !o) throw new le(`Graph.dropEdge: could not find the "${i}" -> "${u}" edge in the graph.`);
          } else if (a = "" + a, o = this._edges.get(a), !o) throw new le(`Graph.dropEdge: could not find the "${a}" edge in the graph.`);
          return Ca(this, o), this;
        }
        dropDirectedEdge(a, o) {
          if (arguments.length < 2) throw new Ee("Graph.dropDirectedEdge: it does not make sense to try and drop a directed edge by key. What if the edge with this key is undirected? Use #.dropEdge for this purpose instead.");
          if (this.multi) throw new Ee("Graph.dropDirectedEdge: cannot use a {source,target} combo when dropping an edge in a MultiGraph since we cannot infer the one you want to delete as there could be multiple ones.");
          a = "" + a, o = "" + o;
          const i = rn(this, a, o, "directed");
          if (!i) throw new le(`Graph.dropDirectedEdge: could not find a "${a}" -> "${o}" edge in the graph.`);
          return Ca(this, i), this;
        }
        dropUndirectedEdge(a, o) {
          if (arguments.length < 2) throw new Ee("Graph.dropUndirectedEdge: it does not make sense to drop a directed edge by key. What if the edge with this key is undirected? Use #.dropEdge for this purpose instead.");
          if (this.multi) throw new Ee("Graph.dropUndirectedEdge: cannot use a {source,target} combo when dropping an edge in a MultiGraph since we cannot infer the one you want to delete as there could be multiple ones.");
          const i = rn(this, a, o, "undirected");
          if (!i) throw new le(`Graph.dropUndirectedEdge: could not find a "${a}" -> "${o}" edge in the graph.`);
          return Ca(this, i), this;
        }
        clear() {
          this._edges.clear(), this._nodes.clear(), this._resetInstanceCounters(), this.emit("cleared");
        }
        clearEdges() {
          const a = this._nodes.values();
          let o;
          for (; o = a.next(), o.done !== true; ) o.value.clear();
          this._edges.clear(), this._resetInstanceCounters(), this.emit("edgesCleared");
        }
        getAttribute(a) {
          return this._attributes[a];
        }
        getAttributes() {
          return this._attributes;
        }
        hasAttribute(a) {
          return this._attributes.hasOwnProperty(a);
        }
        setAttribute(a, o) {
          return this._attributes[a] = o, this.emit("attributesUpdated", {
            type: "set",
            attributes: this._attributes,
            name: a
          }), this;
        }
        updateAttribute(a, o) {
          if (typeof o != "function") throw new se("Graph.updateAttribute: updater should be a function.");
          const i = this._attributes[a];
          return this._attributes[a] = o(i), this.emit("attributesUpdated", {
            type: "set",
            attributes: this._attributes,
            name: a
          }), this;
        }
        removeAttribute(a) {
          return delete this._attributes[a], this.emit("attributesUpdated", {
            type: "remove",
            attributes: this._attributes,
            name: a
          }), this;
        }
        replaceAttributes(a) {
          if (!Tt(a)) throw new se("Graph.replaceAttributes: provided attributes are not a plain object.");
          return this._attributes = a, this.emit("attributesUpdated", {
            type: "replace",
            attributes: this._attributes
          }), this;
        }
        mergeAttributes(a) {
          if (!Tt(a)) throw new se("Graph.mergeAttributes: provided attributes are not a plain object.");
          return gt(this._attributes, a), this.emit("attributesUpdated", {
            type: "merge",
            attributes: this._attributes,
            data: a
          }), this;
        }
        updateAttributes(a) {
          if (typeof a != "function") throw new se("Graph.updateAttributes: provided updater is not a function.");
          return this._attributes = a(this._attributes), this.emit("attributesUpdated", {
            type: "update",
            attributes: this._attributes
          }), this;
        }
        updateEachNodeAttributes(a, o) {
          if (typeof a != "function") throw new se("Graph.updateEachNodeAttributes: expecting an updater function.");
          if (o && !lm(o)) throw new se("Graph.updateEachNodeAttributes: invalid hints. Expecting an object having the following shape: {attributes?: [string]}");
          const i = this._nodes.values();
          let u, c;
          for (; u = i.next(), u.done !== true; ) c = u.value, c.attributes = a(c.key, c.attributes);
          this.emit("eachNodeAttributesUpdated", {
            hints: o || null
          });
        }
        updateEachEdgeAttributes(a, o) {
          if (typeof a != "function") throw new se("Graph.updateEachEdgeAttributes: expecting an updater function.");
          if (o && !lm(o)) throw new se("Graph.updateEachEdgeAttributes: invalid hints. Expecting an object having the following shape: {attributes?: [string]}");
          const i = this._edges.values();
          let u, c, d, h;
          for (; u = i.next(), u.done !== true; ) c = u.value, d = c.source, h = c.target, c.attributes = a(c.key, c.attributes, d.key, h.key, d.attributes, h.attributes, c.undirected);
          this.emit("eachEdgeAttributesUpdated", {
            hints: o || null
          });
        }
        forEachAdjacencyEntry(a) {
          if (typeof a != "function") throw new se("Graph.forEachAdjacencyEntry: expecting a callback.");
          mo(false, false, false, this, a);
        }
        forEachAdjacencyEntryWithOrphans(a) {
          if (typeof a != "function") throw new se("Graph.forEachAdjacencyEntryWithOrphans: expecting a callback.");
          mo(false, false, true, this, a);
        }
        forEachAssymetricAdjacencyEntry(a) {
          if (typeof a != "function") throw new se("Graph.forEachAssymetricAdjacencyEntry: expecting a callback.");
          mo(false, true, false, this, a);
        }
        forEachAssymetricAdjacencyEntryWithOrphans(a) {
          if (typeof a != "function") throw new se("Graph.forEachAssymetricAdjacencyEntryWithOrphans: expecting a callback.");
          mo(false, true, true, this, a);
        }
        nodes() {
          return Array.from(this._nodes.keys());
        }
        forEachNode(a) {
          if (typeof a != "function") throw new se("Graph.forEachNode: expecting a callback.");
          const o = this._nodes.values();
          let i, u;
          for (; i = o.next(), i.done !== true; ) u = i.value, a(u.key, u.attributes);
        }
        findNode(a) {
          if (typeof a != "function") throw new se("Graph.findNode: expecting a callback.");
          const o = this._nodes.values();
          let i, u;
          for (; i = o.next(), i.done !== true; ) if (u = i.value, a(u.key, u.attributes)) return u.key;
        }
        mapNodes(a) {
          if (typeof a != "function") throw new se("Graph.mapNode: expecting a callback.");
          const o = this._nodes.values();
          let i, u;
          const c = new Array(this.order);
          let d = 0;
          for (; i = o.next(), i.done !== true; ) u = i.value, c[d++] = a(u.key, u.attributes);
          return c;
        }
        someNode(a) {
          if (typeof a != "function") throw new se("Graph.someNode: expecting a callback.");
          const o = this._nodes.values();
          let i, u;
          for (; i = o.next(), i.done !== true; ) if (u = i.value, a(u.key, u.attributes)) return true;
          return false;
        }
        everyNode(a) {
          if (typeof a != "function") throw new se("Graph.everyNode: expecting a callback.");
          const o = this._nodes.values();
          let i, u;
          for (; i = o.next(), i.done !== true; ) if (u = i.value, !a(u.key, u.attributes)) return false;
          return true;
        }
        filterNodes(a) {
          if (typeof a != "function") throw new se("Graph.filterNodes: expecting a callback.");
          const o = this._nodes.values();
          let i, u;
          const c = [];
          for (; i = o.next(), i.done !== true; ) u = i.value, a(u.key, u.attributes) && c.push(u.key);
          return c;
        }
        reduceNodes(a, o) {
          if (typeof a != "function") throw new se("Graph.reduceNodes: expecting a callback.");
          if (arguments.length < 2) throw new se("Graph.reduceNodes: missing initial value. You must provide it because the callback takes more than one argument and we cannot infer the initial value from the first iteration, as you could with a simple array.");
          let i = o;
          const u = this._nodes.values();
          let c, d;
          for (; c = u.next(), c.done !== true; ) d = c.value, i = a(i, d.key, d.attributes);
          return i;
        }
        nodeEntries() {
          const a = this._nodes.values();
          return {
            [Symbol.iterator]() {
              return this;
            },
            next() {
              const o = a.next();
              if (o.done) return o;
              const i = o.value;
              return {
                value: {
                  node: i.key,
                  attributes: i.attributes
                },
                done: false
              };
            }
          };
        }
        export() {
          const a = new Array(this._nodes.size);
          let o = 0;
          this._nodes.forEach((u, c) => {
            a[o++] = uE(c, u);
          });
          const i = new Array(this._edges.size);
          return o = 0, this._edges.forEach((u, c) => {
            i[o++] = sE(this.type, c, u);
          }), {
            options: {
              type: this.type,
              multi: this.multi,
              allowSelfLoops: this.allowSelfLoops
            },
            attributes: this.getAttributes(),
            nodes: a,
            edges: i
          };
        }
        import(a, o = false) {
          if (a instanceof Ac) return a.forEachNode((m, g) => {
            o ? this.mergeNode(m, g) : this.addNode(m, g);
          }), a.forEachEdge((m, g, p, _, E, R, z) => {
            o ? z ? this.mergeUndirectedEdgeWithKey(m, p, _, g) : this.mergeDirectedEdgeWithKey(m, p, _, g) : z ? this.addUndirectedEdgeWithKey(m, p, _, g) : this.addDirectedEdgeWithKey(m, p, _, g);
          }), this;
          if (!Tt(a)) throw new se("Graph.import: invalid argument. Expecting a serialized graph or, alternatively, a Graph instance.");
          if (a.attributes) {
            if (!Tt(a.attributes)) throw new se("Graph.import: invalid attributes. Expecting a plain object.");
            o ? this.mergeAttributes(a.attributes) : this.replaceAttributes(a.attributes);
          }
          let i, u, c, d, h;
          if (a.nodes) {
            if (c = a.nodes, !Array.isArray(c)) throw new se("Graph.import: invalid nodes. Expecting an array.");
            for (i = 0, u = c.length; i < u; i++) {
              d = c[i], cE(d);
              const { key: m, attributes: g } = d;
              o ? this.mergeNode(m, g) : this.addNode(m, g);
            }
          }
          if (a.edges) {
            let m = false;
            if (this.type === "undirected" && (m = true), c = a.edges, !Array.isArray(c)) throw new se("Graph.import: invalid edges. Expecting an array.");
            for (i = 0, u = c.length; i < u; i++) {
              h = c[i], fE(h);
              const { source: g, target: p, attributes: _, undirected: E = m } = h;
              let R;
              "key" in h ? (R = o ? E ? this.mergeUndirectedEdgeWithKey : this.mergeDirectedEdgeWithKey : E ? this.addUndirectedEdgeWithKey : this.addDirectedEdgeWithKey, R.call(this, h.key, g, p, _)) : (R = o ? E ? this.mergeUndirectedEdge : this.mergeDirectedEdge : E ? this.addUndirectedEdge : this.addDirectedEdge, R.call(this, g, p, _));
            }
          }
          return this;
        }
        nullCopy(a) {
          const o = new Ac(gt({}, this._options, a));
          return o.replaceAttributes(gt({}, this.getAttributes())), o;
        }
        emptyCopy(a) {
          const o = this.nullCopy(a);
          return this._nodes.forEach((i, u) => {
            const c = gt({}, i.attributes);
            i = new o.NodeDataClass(u, c), o._nodes.set(u, i);
          }), o;
        }
        copy(a) {
          if (a = a || {}, typeof a.type == "string" && a.type !== this.type && a.type !== "mixed") throw new Ee(`Graph.copy: cannot create an incompatible copy from "${this.type}" type to "${a.type}" because this would mean losing information about the current graph.`);
          if (typeof a.multi == "boolean" && a.multi !== this.multi && a.multi !== true) throw new Ee("Graph.copy: cannot create an incompatible copy by downgrading a multi graph to a simple one because this would mean losing information about the current graph.");
          if (typeof a.allowSelfLoops == "boolean" && a.allowSelfLoops !== this.allowSelfLoops && a.allowSelfLoops !== true) throw new Ee("Graph.copy: cannot create an incompatible copy from a graph allowing self loops to one that does not because this would mean losing information about the current graph.");
          const o = this.emptyCopy(a), i = this._edges.values();
          let u, c;
          for (; u = i.next(), u.done !== true; ) c = u.value, Rv(o, "copy", false, c.undirected, c.key, c.source.key, c.target.key, gt({}, c.attributes));
          return o;
        }
        toJSON() {
          return this.export();
        }
        toString() {
          return "[object Graph]";
        }
        inspect() {
          const a = {};
          this._nodes.forEach((c, d) => {
            a[d] = c.attributes;
          });
          const o = {}, i = {};
          this._edges.forEach((c, d) => {
            const h = c.undirected ? "--" : "->";
            let m = "", g = c.source.key, p = c.target.key, _;
            c.undirected && g > p && (_ = g, g = p, p = _);
            const E = `(${g})${h}(${p})`;
            d.startsWith("geid_") ? this.multi && (typeof i[E] > "u" ? i[E] = 0 : i[E]++, m += `${i[E]}. `) : m += `[${d}]: `, m += E, o[m] = c.attributes;
          });
          const u = {};
          for (const c in this) this.hasOwnProperty(c) && !om.has(c) && typeof this[c] != "function" && typeof c != "symbol" && (u[c] = this[c]);
          return u.attributes = this._attributes, u.nodes = a, u.edges = o, an(u, "constructor", this.constructor), u;
        }
      };
      typeof Symbol < "u" && (We.prototype[Symbol.for("nodejs.util.inspect.custom")] = We.prototype.inspect);
      gE.forEach((l) => {
        [
          "add",
          "merge",
          "update"
        ].forEach((a) => {
          const o = l.name(a), i = a === "add" ? Rv : pE;
          l.generateKey ? We.prototype[o] = function(u, c, d) {
            return i(this, o, true, (l.type || this.type) === "undirected", null, u, c, d, a === "update");
          } : We.prototype[o] = function(u, c, d, h) {
            return i(this, o, false, (l.type || this.type) === "undirected", u, c, d, h, a === "update");
          };
        });
      });
      Aw(We);
      Uw(We);
      Jw(We);
      oE(We);
      class Cv extends We {
        constructor(a) {
          const o = gt({
            type: "directed"
          }, a);
          if ("multi" in o && o.multi !== false) throw new se("DirectedGraph.from: inconsistent indication that the graph should be multi in given options!");
          if (o.type !== "directed") throw new se('DirectedGraph.from: inconsistent "' + o.type + '" type in given options!');
          super(o);
        }
      }
      class Dv extends We {
        constructor(a) {
          const o = gt({
            type: "undirected"
          }, a);
          if ("multi" in o && o.multi !== false) throw new se("UndirectedGraph.from: inconsistent indication that the graph should be multi in given options!");
          if (o.type !== "undirected") throw new se('UndirectedGraph.from: inconsistent "' + o.type + '" type in given options!');
          super(o);
        }
      }
      class Nv extends We {
        constructor(a) {
          const o = gt({
            multi: true
          }, a);
          if ("multi" in o && o.multi !== true) throw new se("MultiGraph.from: inconsistent indication that the graph should be simple in given options!");
          super(o);
        }
      }
      class Qc extends We {
        constructor(a) {
          const o = gt({
            type: "directed",
            multi: true
          }, a);
          if ("multi" in o && o.multi !== true) throw new se("MultiDirectedGraph.from: inconsistent indication that the graph should be simple in given options!");
          if (o.type !== "directed") throw new se('MultiDirectedGraph.from: inconsistent "' + o.type + '" type in given options!');
          super(o);
        }
      }
      class Ov extends We {
        constructor(a) {
          const o = gt({
            type: "undirected",
            multi: true
          }, a);
          if ("multi" in o && o.multi !== true) throw new se("MultiUndirectedGraph.from: inconsistent indication that the graph should be simple in given options!");
          if (o.type !== "undirected") throw new se('MultiUndirectedGraph.from: inconsistent "' + o.type + '" type in given options!');
          super(o);
        }
      }
      function ja(l) {
        l.from = function(a, o) {
          const i = gt({}, a.options, o), u = new l(i);
          return u.import(a), u;
        };
      }
      ja(We);
      ja(Cv);
      ja(Dv);
      ja(Nv);
      ja(Qc);
      ja(Ov);
      We.Graph = We;
      We.DirectedGraph = Cv;
      We.UndirectedGraph = Dv;
      We.MultiGraph = Nv;
      We.MultiDirectedGraph = Qc;
      We.MultiUndirectedGraph = Ov;
      We.InvalidArgumentsGraphError = se;
      We.NotFoundGraphError = le;
      We.UsageGraphError = Ee;
      function yE(l, a) {
        if (typeof l != "object" || !l) return l;
        var o = l[Symbol.toPrimitive];
        if (o !== void 0) {
          var i = o.call(l, a);
          if (typeof i != "object") return i;
          throw new TypeError("@@toPrimitive must return a primitive value.");
        }
        return String(l);
      }
      function Yr(l) {
        var a = yE(l, "string");
        return typeof a == "symbol" ? a : a + "";
      }
      function Dt(l, a) {
        if (!(l instanceof a)) throw new TypeError("Cannot call a class as a function");
      }
      function sm(l, a) {
        for (var o = 0; o < a.length; o++) {
          var i = a[o];
          i.enumerable = i.enumerable || false, i.configurable = true, "value" in i && (i.writable = true), Object.defineProperty(l, Yr(i.key), i);
        }
      }
      function Nt(l, a, o) {
        return a && sm(l.prototype, a), o && sm(l, o), Object.defineProperty(l, "prototype", {
          writable: false
        }), l;
      }
      function za(l) {
        return za = Object.setPrototypeOf ? Object.getPrototypeOf.bind() : function(a) {
          return a.__proto__ || Object.getPrototypeOf(a);
        }, za(l);
      }
      function zv() {
        try {
          var l = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
          }));
        } catch {
        }
        return (zv = function() {
          return !!l;
        })();
      }
      function bE(l) {
        if (l === void 0) throw new ReferenceError("this hasn't been initialised - super() hasn't been called");
        return l;
      }
      function _E(l, a) {
        if (a && (typeof a == "object" || typeof a == "function")) return a;
        if (a !== void 0) throw new TypeError("Derived constructors may only return object or undefined");
        return bE(l);
      }
      function ln(l, a, o) {
        return a = za(a), _E(l, zv() ? Reflect.construct(a, o || [], za(l).constructor) : a.apply(l, o));
      }
      function Rc(l, a) {
        return Rc = Object.setPrototypeOf ? Object.setPrototypeOf.bind() : function(o, i) {
          return o.__proto__ = i, o;
        }, Rc(l, a);
      }
      function on(l, a) {
        if (typeof a != "function" && a !== null) throw new TypeError("Super expression must either be null or a function");
        l.prototype = Object.create(a && a.prototype, {
          constructor: {
            value: l,
            writable: true,
            configurable: true
          }
        }), Object.defineProperty(l, "prototype", {
          writable: false
        }), a && Rc(l, a);
      }
      function wE(l) {
        if (Array.isArray(l)) return l;
      }
      function EE(l, a) {
        var o = l == null ? null : typeof Symbol < "u" && l[Symbol.iterator] || l["@@iterator"];
        if (o != null) {
          var i, u, c, d, h = [], m = true, g = false;
          try {
            if (c = (o = o.call(l)).next, a === 0) {
              if (Object(o) !== o) return;
              m = false;
            } else for (; !(m = (i = c.call(o)).done) && (h.push(i.value), h.length !== a); m = true) ;
          } catch (p) {
            g = true, u = p;
          } finally {
            try {
              if (!m && o.return != null && (d = o.return(), Object(d) !== d)) return;
            } finally {
              if (g) throw u;
            }
          }
          return h;
        }
      }
      function Cc(l, a) {
        (a == null || a > l.length) && (a = l.length);
        for (var o = 0, i = Array(a); o < a; o++) i[o] = l[o];
        return i;
      }
      function kv(l, a) {
        if (l) {
          if (typeof l == "string") return Cc(l, a);
          var o = {}.toString.call(l).slice(8, -1);
          return o === "Object" && l.constructor && (o = l.constructor.name), o === "Map" || o === "Set" ? Array.from(l) : o === "Arguments" || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(o) ? Cc(l, a) : void 0;
        }
      }
      function SE() {
        throw new TypeError(`Invalid attempt to destructure non-iterable instance.
In order to be iterable, non-array objects must have a [Symbol.iterator]() method.`);
      }
      function ka(l, a) {
        return wE(l) || EE(l, a) || kv(l, a) || SE();
      }
      var rc = {
        black: "#000000",
        silver: "#C0C0C0",
        gray: "#808080",
        grey: "#808080",
        white: "#FFFFFF",
        maroon: "#800000",
        red: "#FF0000",
        purple: "#800080",
        fuchsia: "#FF00FF",
        green: "#008000",
        lime: "#00FF00",
        olive: "#808000",
        yellow: "#FFFF00",
        navy: "#000080",
        blue: "#0000FF",
        teal: "#008080",
        aqua: "#00FFFF",
        darkblue: "#00008B",
        mediumblue: "#0000CD",
        darkgreen: "#006400",
        darkcyan: "#008B8B",
        deepskyblue: "#00BFFF",
        darkturquoise: "#00CED1",
        mediumspringgreen: "#00FA9A",
        springgreen: "#00FF7F",
        cyan: "#00FFFF",
        midnightblue: "#191970",
        dodgerblue: "#1E90FF",
        lightseagreen: "#20B2AA",
        forestgreen: "#228B22",
        seagreen: "#2E8B57",
        darkslategray: "#2F4F4F",
        darkslategrey: "#2F4F4F",
        limegreen: "#32CD32",
        mediumseagreen: "#3CB371",
        turquoise: "#40E0D0",
        royalblue: "#4169E1",
        steelblue: "#4682B4",
        darkslateblue: "#483D8B",
        mediumturquoise: "#48D1CC",
        indigo: "#4B0082",
        darkolivegreen: "#556B2F",
        cadetblue: "#5F9EA0",
        cornflowerblue: "#6495ED",
        rebeccapurple: "#663399",
        mediumaquamarine: "#66CDAA",
        dimgray: "#696969",
        dimgrey: "#696969",
        slateblue: "#6A5ACD",
        olivedrab: "#6B8E23",
        slategray: "#708090",
        slategrey: "#708090",
        lightslategray: "#778899",
        lightslategrey: "#778899",
        mediumslateblue: "#7B68EE",
        lawngreen: "#7CFC00",
        chartreuse: "#7FFF00",
        aquamarine: "#7FFFD4",
        skyblue: "#87CEEB",
        lightskyblue: "#87CEFA",
        blueviolet: "#8A2BE2",
        darkred: "#8B0000",
        darkmagenta: "#8B008B",
        saddlebrown: "#8B4513",
        darkseagreen: "#8FBC8F",
        lightgreen: "#90EE90",
        mediumpurple: "#9370DB",
        darkviolet: "#9400D3",
        palegreen: "#98FB98",
        darkorchid: "#9932CC",
        yellowgreen: "#9ACD32",
        sienna: "#A0522D",
        brown: "#A52A2A",
        darkgray: "#A9A9A9",
        darkgrey: "#A9A9A9",
        lightblue: "#ADD8E6",
        greenyellow: "#ADFF2F",
        paleturquoise: "#AFEEEE",
        lightsteelblue: "#B0C4DE",
        powderblue: "#B0E0E6",
        firebrick: "#B22222",
        darkgoldenrod: "#B8860B",
        mediumorchid: "#BA55D3",
        rosybrown: "#BC8F8F",
        darkkhaki: "#BDB76B",
        mediumvioletred: "#C71585",
        indianred: "#CD5C5C",
        peru: "#CD853F",
        chocolate: "#D2691E",
        tan: "#D2B48C",
        lightgray: "#D3D3D3",
        lightgrey: "#D3D3D3",
        thistle: "#D8BFD8",
        orchid: "#DA70D6",
        goldenrod: "#DAA520",
        palevioletred: "#DB7093",
        crimson: "#DC143C",
        gainsboro: "#DCDCDC",
        plum: "#DDA0DD",
        burlywood: "#DEB887",
        lightcyan: "#E0FFFF",
        lavender: "#E6E6FA",
        darksalmon: "#E9967A",
        violet: "#EE82EE",
        palegoldenrod: "#EEE8AA",
        lightcoral: "#F08080",
        khaki: "#F0E68C",
        aliceblue: "#F0F8FF",
        honeydew: "#F0FFF0",
        azure: "#F0FFFF",
        sandybrown: "#F4A460",
        wheat: "#F5DEB3",
        beige: "#F5F5DC",
        whitesmoke: "#F5F5F5",
        mintcream: "#F5FFFA",
        ghostwhite: "#F8F8FF",
        salmon: "#FA8072",
        antiquewhite: "#FAEBD7",
        linen: "#FAF0E6",
        lightgoldenrodyellow: "#FAFAD2",
        oldlace: "#FDF5E6",
        magenta: "#FF00FF",
        deeppink: "#FF1493",
        orangered: "#FF4500",
        tomato: "#FF6347",
        hotpink: "#FF69B4",
        coral: "#FF7F50",
        darkorange: "#FF8C00",
        lightsalmon: "#FFA07A",
        orange: "#FFA500",
        lightpink: "#FFB6C1",
        pink: "#FFC0CB",
        gold: "#FFD700",
        peachpuff: "#FFDAB9",
        navajowhite: "#FFDEAD",
        moccasin: "#FFE4B5",
        bisque: "#FFE4C4",
        mistyrose: "#FFE4E1",
        blanchedalmond: "#FFEBCD",
        papayawhip: "#FFEFD5",
        lavenderblush: "#FFF0F5",
        seashell: "#FFF5EE",
        cornsilk: "#FFF8DC",
        lemonchiffon: "#FFFACD",
        floralwhite: "#FFFAF0",
        snow: "#FFFAFA",
        lightyellow: "#FFFFE0",
        ivory: "#FFFFF0"
      }, Mv = new Int8Array(4), xo = new Int32Array(Mv.buffer, 0, 1), Lv = new Float32Array(Mv.buffer, 0, 1), xE = /^\s*rgba?\s*\(/, TE = /^\s*rgba?\s*\(\s*([0-9]*)\s*,\s*([0-9]*)\s*,\s*([0-9]*)(?:\s*,\s*(.*)?)?\)\s*$/;
      function AE(l) {
        var a = 0, o = 0, i = 0, u = 1;
        if (l[0] === "#") l.length === 4 ? (a = parseInt(l.charAt(1) + l.charAt(1), 16), o = parseInt(l.charAt(2) + l.charAt(2), 16), i = parseInt(l.charAt(3) + l.charAt(3), 16)) : (a = parseInt(l.charAt(1) + l.charAt(2), 16), o = parseInt(l.charAt(3) + l.charAt(4), 16), i = parseInt(l.charAt(5) + l.charAt(6), 16)), l.length === 9 && (u = parseInt(l.charAt(7) + l.charAt(8), 16) / 255);
        else if (xE.test(l)) {
          var c = l.match(TE);
          c && (a = +c[1], o = +c[2], i = +c[3], c[4] && (u = +c[4]));
        }
        return {
          r: a,
          g: o,
          b: i,
          a: u
        };
      }
      var Oa = {};
      for (var vo in rc) Oa[vo] = Hi(rc[vo]), Oa[rc[vo]] = Oa[vo];
      function Gv(l, a, o, i, u) {
        return xo[0] = i << 24 | o << 16 | a << 8 | l, xo[0] = xo[0] & 4278190079, Lv[0];
      }
      function Hi(l) {
        if (l = l.toLowerCase(), typeof Oa[l] < "u") return Oa[l];
        var a = AE(l), o = a.r, i = a.g, u = a.b, c = a.a;
        c = c * 255 | 0;
        var d = Gv(o, i, u, c);
        return Oa[l] = d, d;
      }
      function RE(l, a) {
        Lv[0] = Hi(l);
        var o = xo[0], i = o & 255, u = o >> 8 & 255, c = o >> 16 & 255, d = o >> 24 & 255;
        return [
          i,
          u,
          c,
          d
        ];
      }
      var lc = {};
      function Uv(l) {
        if (typeof lc[l] < "u") return lc[l];
        var a = (l & 16711680) >>> 16, o = (l & 65280) >>> 8, i = l & 255, u = 255, c = Gv(a, o, i, u);
        return lc[l] = c, c;
      }
      function cm(l, a, o, i) {
        return o + (a << 8) + (l << 16);
      }
      function fm(l, a, o, i, u, c) {
        var d = Math.floor(o / c * u), h = Math.floor(l.drawingBufferHeight / c - i / c * u), m = new Uint8Array(4);
        l.bindFramebuffer(l.FRAMEBUFFER, a), l.readPixels(d, h, 1, 1, l.RGBA, l.UNSIGNED_BYTE, m);
        var g = ka(m, 4), p = g[0], _ = g[1], E = g[2], R = g[3];
        return [
          p,
          _,
          E,
          R
        ];
      }
      function ee(l, a, o) {
        return (a = Yr(a)) in l ? Object.defineProperty(l, a, {
          value: o,
          enumerable: true,
          configurable: true,
          writable: true
        }) : l[a] = o, l;
      }
      function dm(l, a) {
        var o = Object.keys(l);
        if (Object.getOwnPropertySymbols) {
          var i = Object.getOwnPropertySymbols(l);
          a && (i = i.filter(function(u) {
            return Object.getOwnPropertyDescriptor(l, u).enumerable;
          })), o.push.apply(o, i);
        }
        return o;
      }
      function ve(l) {
        for (var a = 1; a < arguments.length; a++) {
          var o = arguments[a] != null ? arguments[a] : {};
          a % 2 ? dm(Object(o), true).forEach(function(i) {
            ee(l, i, o[i]);
          }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(l, Object.getOwnPropertyDescriptors(o)) : dm(Object(o)).forEach(function(i) {
            Object.defineProperty(l, i, Object.getOwnPropertyDescriptor(o, i));
          });
        }
        return l;
      }
      function CE(l, a) {
        for (; !{}.hasOwnProperty.call(l, a) && (l = za(l)) !== null; ) ;
        return l;
      }
      function Dc() {
        return Dc = typeof Reflect < "u" && Reflect.get ? Reflect.get.bind() : function(l, a, o) {
          var i = CE(l, a);
          if (i) {
            var u = Object.getOwnPropertyDescriptor(i, a);
            return u.get ? u.get.call(arguments.length < 3 ? l : o) : u.value;
          }
        }, Dc.apply(null, arguments);
      }
      function jv(l, a, o, i) {
        var u = Dc(za(l.prototype), a, o);
        return typeof u == "function" ? function(c) {
          return u.apply(o, c);
        } : u;
      }
      function DE(l) {
        return l.normalized ? 1 : l.size;
      }
      function oc(l) {
        var a = 0;
        return l.forEach(function(o) {
          return a += DE(o);
        }), a;
      }
      function Bv(l, a, o) {
        var i = l === "VERTEX" ? a.VERTEX_SHADER : a.FRAGMENT_SHADER, u = a.createShader(i);
        if (u === null) throw new Error("loadShader: error while creating the shader");
        a.shaderSource(u, o), a.compileShader(u);
        var c = a.getShaderParameter(u, a.COMPILE_STATUS);
        if (!c) {
          var d = a.getShaderInfoLog(u);
          throw a.deleteShader(u), new Error(`loadShader: error while compiling the shader:
`.concat(d, `
`).concat(o));
        }
        return u;
      }
      function NE(l, a) {
        return Bv("VERTEX", l, a);
      }
      function OE(l, a) {
        return Bv("FRAGMENT", l, a);
      }
      function zE(l, a) {
        var o = l.createProgram();
        if (o === null) throw new Error("loadProgram: error while creating the program.");
        var i, u;
        for (i = 0, u = a.length; i < u; i++) l.attachShader(o, a[i]);
        l.linkProgram(o);
        var c = l.getProgramParameter(o, l.LINK_STATUS);
        if (!c) throw l.deleteProgram(o), new Error("loadProgram: error while linking the program.");
        return o;
      }
      function hm(l) {
        var a = l.gl, o = l.buffer, i = l.program, u = l.vertexShader, c = l.fragmentShader;
        a.deleteShader(u), a.deleteShader(c), a.deleteProgram(i), a.deleteBuffer(o);
      }
      function gm(l) {
        return l % 1 === 0 ? l.toFixed(1) : l.toString();
      }
      var mm = `#define PICKING_MODE
`, kE = ee(ee(ee(ee(ee(ee(ee(ee({}, WebGL2RenderingContext.BOOL, 1), WebGL2RenderingContext.BYTE, 1), WebGL2RenderingContext.UNSIGNED_BYTE, 1), WebGL2RenderingContext.SHORT, 2), WebGL2RenderingContext.UNSIGNED_SHORT, 2), WebGL2RenderingContext.INT, 4), WebGL2RenderingContext.UNSIGNED_INT, 4), WebGL2RenderingContext.FLOAT, 4), Hv = function() {
        function l(a, o, i) {
          Dt(this, l), ee(this, "array", new Float32Array()), ee(this, "constantArray", new Float32Array()), ee(this, "capacity", 0), ee(this, "verticesCount", 0);
          var u = this.getDefinition();
          if (this.VERTICES = u.VERTICES, this.VERTEX_SHADER_SOURCE = u.VERTEX_SHADER_SOURCE, this.FRAGMENT_SHADER_SOURCE = u.FRAGMENT_SHADER_SOURCE, this.UNIFORMS = u.UNIFORMS, this.ATTRIBUTES = u.ATTRIBUTES, this.METHOD = u.METHOD, this.CONSTANT_ATTRIBUTES = "CONSTANT_ATTRIBUTES" in u ? u.CONSTANT_ATTRIBUTES : [], this.CONSTANT_DATA = "CONSTANT_DATA" in u ? u.CONSTANT_DATA : [], this.isInstanced = "CONSTANT_ATTRIBUTES" in u, this.ATTRIBUTES_ITEMS_COUNT = oc(this.ATTRIBUTES), this.STRIDE = this.VERTICES * this.ATTRIBUTES_ITEMS_COUNT, this.renderer = i, this.normalProgram = this.getProgramInfo("normal", a, u.VERTEX_SHADER_SOURCE, u.FRAGMENT_SHADER_SOURCE, null), this.pickProgram = o ? this.getProgramInfo("pick", a, mm + u.VERTEX_SHADER_SOURCE, mm + u.FRAGMENT_SHADER_SOURCE, o) : null, this.isInstanced) {
            var c = oc(this.CONSTANT_ATTRIBUTES);
            if (this.CONSTANT_DATA.length !== this.VERTICES) throw new Error("Program: error while getting constant data (expected ".concat(this.VERTICES, " items, received ").concat(this.CONSTANT_DATA.length, " instead)"));
            this.constantArray = new Float32Array(this.CONSTANT_DATA.length * c);
            for (var d = 0; d < this.CONSTANT_DATA.length; d++) {
              var h = this.CONSTANT_DATA[d];
              if (h.length !== c) throw new Error("Program: error while getting constant data (one vector has ".concat(h.length, " items instead of ").concat(c, ")"));
              for (var m = 0; m < h.length; m++) this.constantArray[d * c + m] = h[m];
            }
            this.STRIDE = this.ATTRIBUTES_ITEMS_COUNT;
          }
        }
        return Nt(l, [
          {
            key: "kill",
            value: function() {
              hm(this.normalProgram), this.pickProgram && (hm(this.pickProgram), this.pickProgram = null);
            }
          },
          {
            key: "getProgramInfo",
            value: function(o, i, u, c, d) {
              var h = this.getDefinition(), m = i.createBuffer();
              if (m === null) throw new Error("Program: error while creating the WebGL buffer.");
              var g = NE(i, u), p = OE(i, c), _ = zE(i, [
                g,
                p
              ]), E = {};
              h.UNIFORMS.forEach(function(H) {
                var Y = i.getUniformLocation(_, H);
                Y && (E[H] = Y);
              });
              var R = {};
              h.ATTRIBUTES.forEach(function(H) {
                R[H.name] = i.getAttribLocation(_, H.name);
              });
              var z;
              if ("CONSTANT_ATTRIBUTES" in h && (h.CONSTANT_ATTRIBUTES.forEach(function(H) {
                R[H.name] = i.getAttribLocation(_, H.name);
              }), z = i.createBuffer(), z === null)) throw new Error("Program: error while creating the WebGL constant buffer.");
              return {
                name: o,
                program: _,
                gl: i,
                frameBuffer: d,
                buffer: m,
                constantBuffer: z || {},
                uniformLocations: E,
                attributeLocations: R,
                isPicking: o === "pick",
                vertexShader: g,
                fragmentShader: p
              };
            }
          },
          {
            key: "bindProgram",
            value: function(o) {
              var i = this, u = 0, c = o.gl, d = o.buffer;
              this.isInstanced ? (c.bindBuffer(c.ARRAY_BUFFER, o.constantBuffer), u = 0, this.CONSTANT_ATTRIBUTES.forEach(function(h) {
                return u += i.bindAttribute(h, o, u, false);
              }), c.bufferData(c.ARRAY_BUFFER, this.constantArray, c.STATIC_DRAW), c.bindBuffer(c.ARRAY_BUFFER, o.buffer), u = 0, this.ATTRIBUTES.forEach(function(h) {
                return u += i.bindAttribute(h, o, u, true);
              }), c.bufferData(c.ARRAY_BUFFER, this.array, c.DYNAMIC_DRAW)) : (c.bindBuffer(c.ARRAY_BUFFER, d), u = 0, this.ATTRIBUTES.forEach(function(h) {
                return u += i.bindAttribute(h, o, u);
              }), c.bufferData(c.ARRAY_BUFFER, this.array, c.DYNAMIC_DRAW)), c.bindBuffer(c.ARRAY_BUFFER, null);
            }
          },
          {
            key: "unbindProgram",
            value: function(o) {
              var i = this;
              this.isInstanced ? (this.CONSTANT_ATTRIBUTES.forEach(function(u) {
                return i.unbindAttribute(u, o, false);
              }), this.ATTRIBUTES.forEach(function(u) {
                return i.unbindAttribute(u, o, true);
              })) : this.ATTRIBUTES.forEach(function(u) {
                return i.unbindAttribute(u, o);
              });
            }
          },
          {
            key: "bindAttribute",
            value: function(o, i, u, c) {
              var d = kE[o.type];
              if (typeof d != "number") throw new Error('Program.bind: yet unsupported attribute type "'.concat(o.type, '"'));
              var h = i.attributeLocations[o.name], m = i.gl;
              if (h !== -1) {
                m.enableVertexAttribArray(h);
                var g = this.isInstanced ? (c ? this.ATTRIBUTES_ITEMS_COUNT : oc(this.CONSTANT_ATTRIBUTES)) * Float32Array.BYTES_PER_ELEMENT : this.ATTRIBUTES_ITEMS_COUNT * Float32Array.BYTES_PER_ELEMENT;
                if (m.vertexAttribPointer(h, o.size, o.type, o.normalized || false, g, u), this.isInstanced && c) if (m instanceof WebGL2RenderingContext) m.vertexAttribDivisor(h, 1);
                else {
                  var p = m.getExtension("ANGLE_instanced_arrays");
                  p && p.vertexAttribDivisorANGLE(h, 1);
                }
              }
              return o.size * d;
            }
          },
          {
            key: "unbindAttribute",
            value: function(o, i, u) {
              var c = i.attributeLocations[o.name], d = i.gl;
              if (c !== -1 && (d.disableVertexAttribArray(c), this.isInstanced && u)) if (d instanceof WebGL2RenderingContext) d.vertexAttribDivisor(c, 0);
              else {
                var h = d.getExtension("ANGLE_instanced_arrays");
                h && h.vertexAttribDivisorANGLE(c, 0);
              }
            }
          },
          {
            key: "reallocate",
            value: function(o) {
              o !== this.capacity && (this.capacity = o, this.verticesCount = this.VERTICES * o, this.array = new Float32Array(this.isInstanced ? this.capacity * this.ATTRIBUTES_ITEMS_COUNT : this.verticesCount * this.ATTRIBUTES_ITEMS_COUNT));
            }
          },
          {
            key: "hasNothingToRender",
            value: function() {
              return this.verticesCount === 0;
            }
          },
          {
            key: "renderProgram",
            value: function(o, i) {
              var u = i.gl, c = i.program;
              u.enable(u.BLEND), u.useProgram(c), this.setUniforms(o, i), this.drawWebGL(this.METHOD, i);
            }
          },
          {
            key: "render",
            value: function(o) {
              this.hasNothingToRender() || (this.pickProgram && (this.pickProgram.gl.viewport(0, 0, o.width * o.pixelRatio / o.downSizingRatio, o.height * o.pixelRatio / o.downSizingRatio), this.bindProgram(this.pickProgram), this.renderProgram(ve(ve({}, o), {}, {
                pixelRatio: o.pixelRatio / o.downSizingRatio
              }), this.pickProgram), this.unbindProgram(this.pickProgram)), this.normalProgram.gl.viewport(0, 0, o.width * o.pixelRatio, o.height * o.pixelRatio), this.bindProgram(this.normalProgram), this.renderProgram(o, this.normalProgram), this.unbindProgram(this.normalProgram));
            }
          },
          {
            key: "drawWebGL",
            value: function(o, i) {
              var u = i.gl, c = i.frameBuffer;
              if (u.bindFramebuffer(u.FRAMEBUFFER, c), !this.isInstanced) u.drawArrays(o, 0, this.verticesCount);
              else if (u instanceof WebGL2RenderingContext) u.drawArraysInstanced(o, 0, this.VERTICES, this.capacity);
              else {
                var d = u.getExtension("ANGLE_instanced_arrays");
                d && d.drawArraysInstancedANGLE(o, 0, this.VERTICES, this.capacity);
              }
            }
          }
        ]);
      }(), Fv = function(l) {
        function a() {
          return Dt(this, a), ln(this, a, arguments);
        }
        return on(a, l), Nt(a, [
          {
            key: "kill",
            value: function() {
              jv(a, "kill", this)([]);
            }
          },
          {
            key: "process",
            value: function(i, u, c) {
              var d = u * this.STRIDE;
              if (c.hidden) {
                for (var h = d + this.STRIDE; d < h; d++) this.array[d] = 0;
                return;
              }
              return this.processVisibleItem(Uv(i), d, c);
            }
          }
        ]);
      }(Hv), Kc = function(l) {
        function a() {
          var o;
          Dt(this, a);
          for (var i = arguments.length, u = new Array(i), c = 0; c < i; c++) u[c] = arguments[c];
          return o = ln(this, a, [].concat(u)), ee(o, "drawLabel", void 0), o;
        }
        return on(a, l), Nt(a, [
          {
            key: "kill",
            value: function() {
              jv(a, "kill", this)([]);
            }
          },
          {
            key: "process",
            value: function(i, u, c, d, h) {
              var m = u * this.STRIDE;
              if (h.hidden || c.hidden || d.hidden) {
                for (var g = m + this.STRIDE; m < g; m++) this.array[m] = 0;
                return;
              }
              return this.processVisibleItem(Uv(i), m, c, d, h);
            }
          }
        ]);
      }(Hv);
      function ME(l, a) {
        return function() {
          function o(i, u, c) {
            Dt(this, o), ee(this, "drawLabel", a), this.programs = l.map(function(d) {
              return new d(i, u, c);
            });
          }
          return Nt(o, [
            {
              key: "reallocate",
              value: function(u) {
                this.programs.forEach(function(c) {
                  return c.reallocate(u);
                });
              }
            },
            {
              key: "process",
              value: function(u, c, d, h, m) {
                this.programs.forEach(function(g) {
                  return g.process(u, c, d, h, m);
                });
              }
            },
            {
              key: "render",
              value: function(u) {
                this.programs.forEach(function(c) {
                  return c.render(u);
                });
              }
            },
            {
              key: "kill",
              value: function() {
                this.programs.forEach(function(u) {
                  return u.kill();
                });
              }
            }
          ]);
        }();
      }
      function LE(l, a, o, i, u) {
        var c = u.edgeLabelSize, d = u.edgeLabelFont, h = u.edgeLabelWeight, m = u.edgeLabelColor.attribute ? a[u.edgeLabelColor.attribute] || u.edgeLabelColor.color || "#000" : u.edgeLabelColor.color, g = a.label;
        if (g) {
          l.fillStyle = m, l.font = "".concat(h, " ").concat(c, "px ").concat(d);
          var p = o.size, _ = i.size, E = o.x, R = o.y, z = i.x, H = i.y, Y = (E + z) / 2, te = (R + H) / 2, J = z - E, I = H - R, G = Math.sqrt(J * J + I * I);
          if (!(G < p + _)) {
            E += J * p / G, R += I * p / G, z -= J * _ / G, H -= I * _ / G, Y = (E + z) / 2, te = (R + H) / 2, J = z - E, I = H - R, G = Math.sqrt(J * J + I * I);
            var B = l.measureText(g).width;
            if (B > G) {
              var Z = "\u2026";
              for (g = g + Z, B = l.measureText(g).width; B > G && g.length > 1; ) g = g.slice(0, -2) + Z, B = l.measureText(g).width;
              if (g.length < 4) return;
            }
            var T;
            J > 0 ? I > 0 ? T = Math.acos(J / G) : T = Math.asin(I / G) : I > 0 ? T = Math.acos(J / G) + Math.PI : T = Math.asin(J / G) + Math.PI / 2, l.save(), l.translate(Y, te), l.rotate(T), l.fillText(g, -B / 2, a.size / 2 + c), l.restore();
          }
        }
      }
      function qv(l, a, o) {
        if (a.label) {
          var i = o.labelSize, u = o.labelFont, c = o.labelWeight, d = o.labelColor.attribute ? a[o.labelColor.attribute] || o.labelColor.color || "#000" : o.labelColor.color;
          l.fillStyle = d, l.font = "".concat(c, " ").concat(i, "px ").concat(u), l.fillText(a.label, a.x + a.size + 3, a.y + i / 3);
        }
      }
      function GE(l, a, o) {
        var i = o.labelSize, u = o.labelFont, c = o.labelWeight;
        l.font = "".concat(c, " ").concat(i, "px ").concat(u), l.fillStyle = "#FFF", l.shadowOffsetX = 0, l.shadowOffsetY = 0, l.shadowBlur = 8, l.shadowColor = "#000";
        var d = 2;
        if (typeof a.label == "string") {
          var h = l.measureText(a.label).width, m = Math.round(h + 5), g = Math.round(i + 2 * d), p = Math.max(a.size, i / 2) + d, _ = Math.asin(g / 2 / p), E = Math.sqrt(Math.abs(Math.pow(p, 2) - Math.pow(g / 2, 2)));
          l.beginPath(), l.moveTo(a.x + E, a.y + g / 2), l.lineTo(a.x + p + m, a.y + g / 2), l.lineTo(a.x + p + m, a.y - g / 2), l.lineTo(a.x + E, a.y - g / 2), l.arc(a.x, a.y, p, _, -_), l.closePath(), l.fill();
        } else l.beginPath(), l.arc(a.x, a.y, a.size + d, 0, Math.PI * 2), l.closePath(), l.fill();
        l.shadowOffsetX = 0, l.shadowOffsetY = 0, l.shadowBlur = 0, qv(l, a, o);
      }
      var UE = `
precision highp float;

varying vec4 v_color;
varying vec2 v_diffVector;
varying float v_radius;

uniform float u_correctionRatio;

const vec4 transparent = vec4(0.0, 0.0, 0.0, 0.0);

void main(void) {
  float border = u_correctionRatio * 2.0;
  float dist = length(v_diffVector) - v_radius + border;

  // No antialiasing for picking mode:
  #ifdef PICKING_MODE
  if (dist > border)
    gl_FragColor = transparent;
  else
    gl_FragColor = v_color;

  #else
  float t = 0.0;
  if (dist > border)
    t = 1.0;
  else if (dist > 0.0)
    t = dist / border;

  gl_FragColor = mix(v_color, transparent, t);
  #endif
}
`, jE = UE, BE = `
attribute vec4 a_id;
attribute vec4 a_color;
attribute vec2 a_position;
attribute float a_size;
attribute float a_angle;

uniform mat3 u_matrix;
uniform float u_sizeRatio;
uniform float u_correctionRatio;

varying vec4 v_color;
varying vec2 v_diffVector;
varying float v_radius;
varying float v_border;

const float bias = 255.0 / 254.0;

void main() {
  float size = a_size * u_correctionRatio / u_sizeRatio * 4.0;
  vec2 diffVector = size * vec2(cos(a_angle), sin(a_angle));
  vec2 position = a_position + diffVector;
  gl_Position = vec4(
    (u_matrix * vec3(position, 1)).xy,
    0,
    1
  );

  v_diffVector = diffVector;
  v_radius = size / 2.0;

  #ifdef PICKING_MODE
  // For picking mode, we use the ID as the color:
  v_color = a_id;
  #else
  // For normal mode, we use the color:
  v_color = a_color;
  #endif

  v_color.a *= bias;
}
`, HE = BE, Vv = WebGLRenderingContext, vm = Vv.UNSIGNED_BYTE, uc = Vv.FLOAT, FE = [
        "u_sizeRatio",
        "u_correctionRatio",
        "u_matrix"
      ], Oo = function(l) {
        function a() {
          return Dt(this, a), ln(this, a, arguments);
        }
        return on(a, l), Nt(a, [
          {
            key: "getDefinition",
            value: function() {
              return {
                VERTICES: 3,
                VERTEX_SHADER_SOURCE: HE,
                FRAGMENT_SHADER_SOURCE: jE,
                METHOD: WebGLRenderingContext.TRIANGLES,
                UNIFORMS: FE,
                ATTRIBUTES: [
                  {
                    name: "a_position",
                    size: 2,
                    type: uc
                  },
                  {
                    name: "a_size",
                    size: 1,
                    type: uc
                  },
                  {
                    name: "a_color",
                    size: 4,
                    type: vm,
                    normalized: true
                  },
                  {
                    name: "a_id",
                    size: 4,
                    type: vm,
                    normalized: true
                  }
                ],
                CONSTANT_ATTRIBUTES: [
                  {
                    name: "a_angle",
                    size: 1,
                    type: uc
                  }
                ],
                CONSTANT_DATA: [
                  [
                    a.ANGLE_1
                  ],
                  [
                    a.ANGLE_2
                  ],
                  [
                    a.ANGLE_3
                  ]
                ]
              };
            }
          },
          {
            key: "processVisibleItem",
            value: function(i, u, c) {
              var d = this.array, h = Hi(c.color);
              d[u++] = c.x, d[u++] = c.y, d[u++] = c.size, d[u++] = h, d[u++] = i;
            }
          },
          {
            key: "setUniforms",
            value: function(i, u) {
              var c = u.gl, d = u.uniformLocations, h = d.u_sizeRatio, m = d.u_correctionRatio, g = d.u_matrix;
              c.uniform1f(m, i.correctionRatio), c.uniform1f(h, i.sizeRatio), c.uniformMatrix3fv(g, false, i.matrix);
            }
          }
        ]);
      }(Fv);
      ee(Oo, "ANGLE_1", 0);
      ee(Oo, "ANGLE_2", 2 * Math.PI / 3);
      ee(Oo, "ANGLE_3", 4 * Math.PI / 3);
      var qE = `
precision mediump float;

varying vec4 v_color;

void main(void) {
  gl_FragColor = v_color;
}
`, VE = qE, YE = `
attribute vec2 a_position;
attribute vec2 a_normal;
attribute float a_radius;
attribute vec3 a_barycentric;

#ifdef PICKING_MODE
attribute vec4 a_id;
#else
attribute vec4 a_color;
#endif

uniform mat3 u_matrix;
uniform float u_sizeRatio;
uniform float u_correctionRatio;
uniform float u_minEdgeThickness;
uniform float u_lengthToThicknessRatio;
uniform float u_widenessToThicknessRatio;

varying vec4 v_color;

const float bias = 255.0 / 254.0;

void main() {
  float minThickness = u_minEdgeThickness;

  float normalLength = length(a_normal);
  vec2 unitNormal = a_normal / normalLength;

  // These first computations are taken from edge.vert.glsl and
  // edge.clamped.vert.glsl. Please read it to get better comments on what's
  // happening:
  float pixelsThickness = max(normalLength / u_sizeRatio, minThickness);
  float webGLThickness = pixelsThickness * u_correctionRatio;
  float webGLNodeRadius = a_radius * 2.0 * u_correctionRatio / u_sizeRatio;
  float webGLArrowHeadLength = webGLThickness * u_lengthToThicknessRatio * 2.0;
  float webGLArrowHeadThickness = webGLThickness * u_widenessToThicknessRatio;

  float da = a_barycentric.x;
  float db = a_barycentric.y;
  float dc = a_barycentric.z;

  vec2 delta = vec2(
      da * (webGLNodeRadius * unitNormal.y)
    + db * ((webGLNodeRadius + webGLArrowHeadLength) * unitNormal.y + webGLArrowHeadThickness * unitNormal.x)
    + dc * ((webGLNodeRadius + webGLArrowHeadLength) * unitNormal.y - webGLArrowHeadThickness * unitNormal.x),

      da * (-webGLNodeRadius * unitNormal.x)
    + db * (-(webGLNodeRadius + webGLArrowHeadLength) * unitNormal.x + webGLArrowHeadThickness * unitNormal.y)
    + dc * (-(webGLNodeRadius + webGLArrowHeadLength) * unitNormal.x - webGLArrowHeadThickness * unitNormal.y)
  );

  vec2 position = (u_matrix * vec3(a_position + delta, 1)).xy;

  gl_Position = vec4(position, 0, 1);

  #ifdef PICKING_MODE
  // For picking mode, we use the ID as the color:
  v_color = a_id;
  #else
  // For normal mode, we use the color:
  v_color = a_color;
  #endif

  v_color.a *= bias;
}
`, $E = YE, Yv = WebGLRenderingContext, pm = Yv.UNSIGNED_BYTE, po = Yv.FLOAT, XE = [
        "u_matrix",
        "u_sizeRatio",
        "u_correctionRatio",
        "u_minEdgeThickness",
        "u_lengthToThicknessRatio",
        "u_widenessToThicknessRatio"
      ], $v = {
        extremity: "target",
        lengthToThicknessRatio: 2.5,
        widenessToThicknessRatio: 2
      };
      function Xv(l) {
        var a = ve(ve({}, $v), l || {});
        return function(o) {
          function i() {
            return Dt(this, i), ln(this, i, arguments);
          }
          return on(i, o), Nt(i, [
            {
              key: "getDefinition",
              value: function() {
                return {
                  VERTICES: 3,
                  VERTEX_SHADER_SOURCE: $E,
                  FRAGMENT_SHADER_SOURCE: VE,
                  METHOD: WebGLRenderingContext.TRIANGLES,
                  UNIFORMS: XE,
                  ATTRIBUTES: [
                    {
                      name: "a_position",
                      size: 2,
                      type: po
                    },
                    {
                      name: "a_normal",
                      size: 2,
                      type: po
                    },
                    {
                      name: "a_radius",
                      size: 1,
                      type: po
                    },
                    {
                      name: "a_color",
                      size: 4,
                      type: pm,
                      normalized: true
                    },
                    {
                      name: "a_id",
                      size: 4,
                      type: pm,
                      normalized: true
                    }
                  ],
                  CONSTANT_ATTRIBUTES: [
                    {
                      name: "a_barycentric",
                      size: 3,
                      type: po
                    }
                  ],
                  CONSTANT_DATA: [
                    [
                      1,
                      0,
                      0
                    ],
                    [
                      0,
                      1,
                      0
                    ],
                    [
                      0,
                      0,
                      1
                    ]
                  ]
                };
              }
            },
            {
              key: "processVisibleItem",
              value: function(c, d, h, m, g) {
                if (a.extremity === "source") {
                  var p = [
                    m,
                    h
                  ];
                  h = p[0], m = p[1];
                }
                var _ = g.size || 1, E = m.size || 1, R = h.x, z = h.y, H = m.x, Y = m.y, te = Hi(g.color), J = H - R, I = Y - z, G = J * J + I * I, B = 0, Z = 0;
                G && (G = 1 / Math.sqrt(G), B = -I * G * _, Z = J * G * _);
                var T = this.array;
                T[d++] = H, T[d++] = Y, T[d++] = -B, T[d++] = -Z, T[d++] = E, T[d++] = te, T[d++] = c;
              }
            },
            {
              key: "setUniforms",
              value: function(c, d) {
                var h = d.gl, m = d.uniformLocations, g = m.u_matrix, p = m.u_sizeRatio, _ = m.u_correctionRatio, E = m.u_minEdgeThickness, R = m.u_lengthToThicknessRatio, z = m.u_widenessToThicknessRatio;
                h.uniformMatrix3fv(g, false, c.matrix), h.uniform1f(p, c.sizeRatio), h.uniform1f(_, c.correctionRatio), h.uniform1f(E, c.minEdgeThickness), h.uniform1f(R, a.lengthToThicknessRatio), h.uniform1f(z, a.widenessToThicknessRatio);
              }
            }
          ]);
        }(Kc);
      }
      Xv();
      var ZE = `
precision mediump float;

varying vec4 v_color;
varying vec2 v_normal;
varying float v_thickness;
varying float v_feather;

const vec4 transparent = vec4(0.0, 0.0, 0.0, 0.0);

void main(void) {
  // We only handle antialiasing for normal mode:
  #ifdef PICKING_MODE
  gl_FragColor = v_color;
  #else
  float dist = length(v_normal) * v_thickness;

  float t = smoothstep(
    v_thickness - v_feather,
    v_thickness,
    dist
  );

  gl_FragColor = mix(v_color, transparent, t);
  #endif
}
`, Zv = ZE, QE = `
attribute vec4 a_id;
attribute vec4 a_color;
attribute vec2 a_normal;
attribute float a_normalCoef;
attribute vec2 a_positionStart;
attribute vec2 a_positionEnd;
attribute float a_positionCoef;
attribute float a_radius;
attribute float a_radiusCoef;

uniform mat3 u_matrix;
uniform float u_zoomRatio;
uniform float u_sizeRatio;
uniform float u_pixelRatio;
uniform float u_correctionRatio;
uniform float u_minEdgeThickness;
uniform float u_lengthToThicknessRatio;
uniform float u_feather;

varying vec4 v_color;
varying vec2 v_normal;
varying float v_thickness;
varying float v_feather;

const float bias = 255.0 / 254.0;

void main() {
  float minThickness = u_minEdgeThickness;

  float radius = a_radius * a_radiusCoef;
  vec2 normal = a_normal * a_normalCoef;
  vec2 position = a_positionStart * (1.0 - a_positionCoef) + a_positionEnd * a_positionCoef;

  float normalLength = length(normal);
  vec2 unitNormal = normal / normalLength;

  // These first computations are taken from edge.vert.glsl. Please read it to
  // get better comments on what's happening:
  float pixelsThickness = max(normalLength, minThickness * u_sizeRatio);
  float webGLThickness = pixelsThickness * u_correctionRatio / u_sizeRatio;

  // Here, we move the point to leave space for the arrow head:
  float direction = sign(radius);
  float webGLNodeRadius = direction * radius * 2.0 * u_correctionRatio / u_sizeRatio;
  float webGLArrowHeadLength = webGLThickness * u_lengthToThicknessRatio * 2.0;

  vec2 compensationVector = vec2(-direction * unitNormal.y, direction * unitNormal.x) * (webGLNodeRadius + webGLArrowHeadLength);

  // Here is the proper position of the vertex
  gl_Position = vec4((u_matrix * vec3(position + unitNormal * webGLThickness + compensationVector, 1)).xy, 0, 1);

  v_thickness = webGLThickness / u_zoomRatio;

  v_normal = unitNormal;

  v_feather = u_feather * u_correctionRatio / u_zoomRatio / u_pixelRatio * 2.0;

  #ifdef PICKING_MODE
  // For picking mode, we use the ID as the color:
  v_color = a_id;
  #else
  // For normal mode, we use the color:
  v_color = a_color;
  #endif

  v_color.a *= bias;
}
`, KE = QE, Qv = WebGLRenderingContext, ym = Qv.UNSIGNED_BYTE, Bi = Qv.FLOAT, PE = [
        "u_matrix",
        "u_zoomRatio",
        "u_sizeRatio",
        "u_correctionRatio",
        "u_pixelRatio",
        "u_feather",
        "u_minEdgeThickness",
        "u_lengthToThicknessRatio"
      ], IE = {
        lengthToThicknessRatio: $v.lengthToThicknessRatio
      };
      function Kv(l) {
        var a = ve(ve({}, IE), l || {});
        return function(o) {
          function i() {
            return Dt(this, i), ln(this, i, arguments);
          }
          return on(i, o), Nt(i, [
            {
              key: "getDefinition",
              value: function() {
                return {
                  VERTICES: 6,
                  VERTEX_SHADER_SOURCE: KE,
                  FRAGMENT_SHADER_SOURCE: Zv,
                  METHOD: WebGLRenderingContext.TRIANGLES,
                  UNIFORMS: PE,
                  ATTRIBUTES: [
                    {
                      name: "a_positionStart",
                      size: 2,
                      type: Bi
                    },
                    {
                      name: "a_positionEnd",
                      size: 2,
                      type: Bi
                    },
                    {
                      name: "a_normal",
                      size: 2,
                      type: Bi
                    },
                    {
                      name: "a_color",
                      size: 4,
                      type: ym,
                      normalized: true
                    },
                    {
                      name: "a_id",
                      size: 4,
                      type: ym,
                      normalized: true
                    },
                    {
                      name: "a_radius",
                      size: 1,
                      type: Bi
                    }
                  ],
                  CONSTANT_ATTRIBUTES: [
                    {
                      name: "a_positionCoef",
                      size: 1,
                      type: Bi
                    },
                    {
                      name: "a_normalCoef",
                      size: 1,
                      type: Bi
                    },
                    {
                      name: "a_radiusCoef",
                      size: 1,
                      type: Bi
                    }
                  ],
                  CONSTANT_DATA: [
                    [
                      0,
                      1,
                      0
                    ],
                    [
                      0,
                      -1,
                      0
                    ],
                    [
                      1,
                      1,
                      1
                    ],
                    [
                      1,
                      1,
                      1
                    ],
                    [
                      0,
                      -1,
                      0
                    ],
                    [
                      1,
                      -1,
                      -1
                    ]
                  ]
                };
              }
            },
            {
              key: "processVisibleItem",
              value: function(c, d, h, m, g) {
                var p = g.size || 1, _ = h.x, E = h.y, R = m.x, z = m.y, H = Hi(g.color), Y = R - _, te = z - E, J = m.size || 1, I = Y * Y + te * te, G = 0, B = 0;
                I && (I = 1 / Math.sqrt(I), G = -te * I * p, B = Y * I * p);
                var Z = this.array;
                Z[d++] = _, Z[d++] = E, Z[d++] = R, Z[d++] = z, Z[d++] = G, Z[d++] = B, Z[d++] = H, Z[d++] = c, Z[d++] = J;
              }
            },
            {
              key: "setUniforms",
              value: function(c, d) {
                var h = d.gl, m = d.uniformLocations, g = m.u_matrix, p = m.u_zoomRatio, _ = m.u_feather, E = m.u_pixelRatio, R = m.u_correctionRatio, z = m.u_sizeRatio, H = m.u_minEdgeThickness, Y = m.u_lengthToThicknessRatio;
                h.uniformMatrix3fv(g, false, c.matrix), h.uniform1f(p, c.zoomRatio), h.uniform1f(z, c.sizeRatio), h.uniform1f(R, c.correctionRatio), h.uniform1f(E, c.pixelRatio), h.uniform1f(_, c.antiAliasingFeather), h.uniform1f(H, c.minEdgeThickness), h.uniform1f(Y, a.lengthToThicknessRatio);
              }
            }
          ]);
        }(Kc);
      }
      Kv();
      function Pv(l) {
        return ME([
          Kv(l),
          Xv(l)
        ]);
      }
      var WE = Pv(), JE = WE, eS = `
attribute vec4 a_id;
attribute vec4 a_color;
attribute vec2 a_normal;
attribute float a_normalCoef;
attribute vec2 a_positionStart;
attribute vec2 a_positionEnd;
attribute float a_positionCoef;

uniform mat3 u_matrix;
uniform float u_sizeRatio;
uniform float u_zoomRatio;
uniform float u_pixelRatio;
uniform float u_correctionRatio;
uniform float u_minEdgeThickness;
uniform float u_feather;

varying vec4 v_color;
varying vec2 v_normal;
varying float v_thickness;
varying float v_feather;

const float bias = 255.0 / 254.0;

void main() {
  float minThickness = u_minEdgeThickness;

  vec2 normal = a_normal * a_normalCoef;
  vec2 position = a_positionStart * (1.0 - a_positionCoef) + a_positionEnd * a_positionCoef;

  float normalLength = length(normal);
  vec2 unitNormal = normal / normalLength;

  // We require edges to be at least "minThickness" pixels thick *on screen*
  // (so we need to compensate the size ratio):
  float pixelsThickness = max(normalLength, minThickness * u_sizeRatio);

  // Then, we need to retrieve the normalized thickness of the edge in the WebGL
  // referential (in a ([0, 1], [0, 1]) space), using our "magic" correction
  // ratio:
  float webGLThickness = pixelsThickness * u_correctionRatio / u_sizeRatio;

  // Here is the proper position of the vertex
  gl_Position = vec4((u_matrix * vec3(position + unitNormal * webGLThickness, 1)).xy, 0, 1);

  // For the fragment shader though, we need a thickness that takes the "magic"
  // correction ratio into account (as in webGLThickness), but so that the
  // antialiasing effect does not depend on the zoom level. So here's yet
  // another thickness version:
  v_thickness = webGLThickness / u_zoomRatio;

  v_normal = unitNormal;

  v_feather = u_feather * u_correctionRatio / u_zoomRatio / u_pixelRatio * 2.0;

  #ifdef PICKING_MODE
  // For picking mode, we use the ID as the color:
  v_color = a_id;
  #else
  // For normal mode, we use the color:
  v_color = a_color;
  #endif

  v_color.a *= bias;
}
`, tS = eS, Iv = WebGLRenderingContext, bm = Iv.UNSIGNED_BYTE, Hr = Iv.FLOAT, nS = [
        "u_matrix",
        "u_zoomRatio",
        "u_sizeRatio",
        "u_correctionRatio",
        "u_pixelRatio",
        "u_feather",
        "u_minEdgeThickness"
      ], iS = function(l) {
        function a() {
          return Dt(this, a), ln(this, a, arguments);
        }
        return on(a, l), Nt(a, [
          {
            key: "getDefinition",
            value: function() {
              return {
                VERTICES: 6,
                VERTEX_SHADER_SOURCE: tS,
                FRAGMENT_SHADER_SOURCE: Zv,
                METHOD: WebGLRenderingContext.TRIANGLES,
                UNIFORMS: nS,
                ATTRIBUTES: [
                  {
                    name: "a_positionStart",
                    size: 2,
                    type: Hr
                  },
                  {
                    name: "a_positionEnd",
                    size: 2,
                    type: Hr
                  },
                  {
                    name: "a_normal",
                    size: 2,
                    type: Hr
                  },
                  {
                    name: "a_color",
                    size: 4,
                    type: bm,
                    normalized: true
                  },
                  {
                    name: "a_id",
                    size: 4,
                    type: bm,
                    normalized: true
                  }
                ],
                CONSTANT_ATTRIBUTES: [
                  {
                    name: "a_positionCoef",
                    size: 1,
                    type: Hr
                  },
                  {
                    name: "a_normalCoef",
                    size: 1,
                    type: Hr
                  }
                ],
                CONSTANT_DATA: [
                  [
                    0,
                    1
                  ],
                  [
                    0,
                    -1
                  ],
                  [
                    1,
                    1
                  ],
                  [
                    1,
                    1
                  ],
                  [
                    0,
                    -1
                  ],
                  [
                    1,
                    -1
                  ]
                ]
              };
            }
          },
          {
            key: "processVisibleItem",
            value: function(i, u, c, d, h) {
              var m = h.size || 1, g = c.x, p = c.y, _ = d.x, E = d.y, R = Hi(h.color), z = _ - g, H = E - p, Y = z * z + H * H, te = 0, J = 0;
              Y && (Y = 1 / Math.sqrt(Y), te = -H * Y * m, J = z * Y * m);
              var I = this.array;
              I[u++] = g, I[u++] = p, I[u++] = _, I[u++] = E, I[u++] = te, I[u++] = J, I[u++] = R, I[u++] = i;
            }
          },
          {
            key: "setUniforms",
            value: function(i, u) {
              var c = u.gl, d = u.uniformLocations, h = d.u_matrix, m = d.u_zoomRatio, g = d.u_feather, p = d.u_pixelRatio, _ = d.u_correctionRatio, E = d.u_sizeRatio, R = d.u_minEdgeThickness;
              c.uniformMatrix3fv(h, false, i.matrix), c.uniform1f(m, i.zoomRatio), c.uniform1f(E, i.sizeRatio), c.uniform1f(_, i.correctionRatio), c.uniform1f(p, i.pixelRatio), c.uniform1f(g, i.antiAliasingFeather), c.uniform1f(R, i.minEdgeThickness);
            }
          }
        ]);
      }(Kc), Pc = function(l) {
        function a() {
          var o;
          return Dt(this, a), o = ln(this, a), o.rawEmitter = o, o;
        }
        return on(a, l), Nt(a);
      }(yv.EventEmitter), sc, _m;
      function Pr() {
        return _m || (_m = 1, sc = function(a) {
          return a !== null && typeof a == "object" && typeof a.addUndirectedEdgeWithKey == "function" && typeof a.dropNode == "function" && typeof a.multi == "boolean";
        }), sc;
      }
      var aS = Pr();
      const rS = Lc(aS);
      var lS = function(a) {
        return a;
      }, oS = function(a) {
        return a * a;
      }, uS = function(a) {
        return a * (2 - a);
      }, sS = function(a) {
        return (a *= 2) < 1 ? 0.5 * a * a : -0.5 * (--a * (a - 2) - 1);
      }, cS = function(a) {
        return a * a * a;
      }, fS = function(a) {
        return --a * a * a + 1;
      }, dS = function(a) {
        return (a *= 2) < 1 ? 0.5 * a * a * a : 0.5 * ((a -= 2) * a * a + 2);
      }, hS = {
        linear: lS,
        quadraticIn: oS,
        quadraticOut: uS,
        quadraticInOut: sS,
        cubicIn: cS,
        cubicOut: fS,
        cubicInOut: dS
      }, gS = {
        easing: "quadraticInOut",
        duration: 150
      };
      function gn() {
        return Float32Array.of(1, 0, 0, 0, 1, 0, 0, 0, 1);
      }
      function yo(l, a, o) {
        return l[0] = a, l[4] = typeof o == "number" ? o : a, l;
      }
      function wm(l, a) {
        var o = Math.sin(a), i = Math.cos(a);
        return l[0] = i, l[1] = o, l[3] = -o, l[4] = i, l;
      }
      function Em(l, a, o) {
        return l[6] = a, l[7] = o, l;
      }
      function yi(l, a) {
        var o = l[0], i = l[1], u = l[2], c = l[3], d = l[4], h = l[5], m = l[6], g = l[7], p = l[8], _ = a[0], E = a[1], R = a[2], z = a[3], H = a[4], Y = a[5], te = a[6], J = a[7], I = a[8];
        return l[0] = _ * o + E * c + R * m, l[1] = _ * i + E * d + R * g, l[2] = _ * u + E * h + R * p, l[3] = z * o + H * c + Y * m, l[4] = z * i + H * d + Y * g, l[5] = z * u + H * h + Y * p, l[6] = te * o + J * c + I * m, l[7] = te * i + J * d + I * g, l[8] = te * u + J * h + I * p, l;
      }
      function Nc(l, a) {
        var o = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : 1, i = l[0], u = l[1], c = l[3], d = l[4], h = l[6], m = l[7], g = a.x, p = a.y;
        return {
          x: g * i + p * c + h * o,
          y: g * u + p * d + m * o
        };
      }
      function mS(l, a) {
        var o = l.height / l.width, i = a.height / a.width;
        return o < 1 && i > 1 || o > 1 && i < 1 ? 1 : Math.min(Math.max(i, 1 / i), Math.max(1 / o, o));
      }
      function Fr(l, a, o, i, u) {
        var c = l.angle, d = l.ratio, h = l.x, m = l.y, g = a.width, p = a.height, _ = gn(), E = Math.min(g, p) - 2 * i, R = mS(a, o);
        return u ? (yi(_, Em(gn(), h, m)), yi(_, yo(gn(), d)), yi(_, wm(gn(), c)), yi(_, yo(gn(), g / E / 2 / R, p / E / 2 / R))) : (yi(_, yo(gn(), 2 * (E / g) * R, 2 * (E / p) * R)), yi(_, wm(gn(), -c)), yi(_, yo(gn(), 1 / d)), yi(_, Em(gn(), -h, -m))), _;
      }
      function vS(l, a, o) {
        var i = Nc(l, {
          x: Math.cos(a.angle),
          y: Math.sin(a.angle)
        }, 0), u = i.x, c = i.y;
        return 1 / Math.sqrt(Math.pow(u, 2) + Math.pow(c, 2)) / o.width;
      }
      function pS(l) {
        if (!l.order) return {
          x: [
            0,
            1
          ],
          y: [
            0,
            1
          ]
        };
        var a = 1 / 0, o = -1 / 0, i = 1 / 0, u = -1 / 0;
        return l.forEachNode(function(c, d) {
          var h = d.x, m = d.y;
          h < a && (a = h), h > o && (o = h), m < i && (i = m), m > u && (u = m);
        }), {
          x: [
            a,
            o
          ],
          y: [
            i,
            u
          ]
        };
      }
      function yS(l) {
        if (!rS(l)) throw new Error("Sigma: invalid graph instance.");
        l.forEachNode(function(a, o) {
          if (!Number.isFinite(o.x) || !Number.isFinite(o.y)) throw new Error("Sigma: Coordinates of node ".concat(a, " are invalid. A node must have a numeric 'x' and 'y' attribute."));
        });
      }
      function bS(l, a, o) {
        var i = document.createElement(l);
        if (a) for (var u in a) i.style[u] = a[u];
        if (o) for (var c in o) i.setAttribute(c, o[c]);
        return i;
      }
      function Sm() {
        return typeof window.devicePixelRatio < "u" ? window.devicePixelRatio : 1;
      }
      function xm(l, a, o) {
        return o.sort(function(i, u) {
          var c = a(i) || 0, d = a(u) || 0;
          return c < d ? -1 : c > d ? 1 : 0;
        });
      }
      function Tm(l) {
        var a = ka(l.x, 2), o = a[0], i = a[1], u = ka(l.y, 2), c = u[0], d = u[1], h = Math.max(i - o, d - c), m = (i + o) / 2, g = (d + c) / 2;
        (h === 0 || Math.abs(h) === 1 / 0 || isNaN(h)) && (h = 1), isNaN(m) && (m = 0), isNaN(g) && (g = 0);
        var p = function(E) {
          return {
            x: 0.5 + (E.x - m) / h,
            y: 0.5 + (E.y - g) / h
          };
        };
        return p.applyTo = function(_) {
          _.x = 0.5 + (_.x - m) / h, _.y = 0.5 + (_.y - g) / h;
        }, p.inverse = function(_) {
          return {
            x: m + h * (_.x - 0.5),
            y: g + h * (_.y - 0.5)
          };
        }, p.ratio = h, p;
      }
      function Oc(l) {
        "@babel/helpers - typeof";
        return Oc = typeof Symbol == "function" && typeof Symbol.iterator == "symbol" ? function(a) {
          return typeof a;
        } : function(a) {
          return a && typeof Symbol == "function" && a.constructor === Symbol && a !== Symbol.prototype ? "symbol" : typeof a;
        }, Oc(l);
      }
      function Am(l, a) {
        var o = a.size;
        if (o !== 0) {
          var i = l.length;
          l.length += o;
          var u = 0;
          a.forEach(function(c) {
            l[i + u] = c, u++;
          });
        }
      }
      function cc(l) {
        l = l || {};
        for (var a = 0, o = arguments.length <= 1 ? 0 : arguments.length - 1; a < o; a++) {
          var i = a + 1 < 1 || arguments.length <= a + 1 ? void 0 : arguments[a + 1];
          i && Object.assign(l, i);
        }
        return l;
      }
      var Ic = {
        hideEdgesOnMove: false,
        hideLabelsOnMove: false,
        renderLabels: true,
        renderEdgeLabels: false,
        enableEdgeEvents: false,
        defaultNodeColor: "#999",
        defaultNodeType: "circle",
        defaultEdgeColor: "#ccc",
        defaultEdgeType: "line",
        labelFont: "Arial",
        labelSize: 14,
        labelWeight: "normal",
        labelColor: {
          color: "#000"
        },
        edgeLabelFont: "Arial",
        edgeLabelSize: 14,
        edgeLabelWeight: "normal",
        edgeLabelColor: {
          attribute: "color"
        },
        stagePadding: 30,
        defaultDrawEdgeLabel: LE,
        defaultDrawNodeLabel: qv,
        defaultDrawNodeHover: GE,
        minEdgeThickness: 1.7,
        antiAliasingFeather: 1,
        dragTimeout: 100,
        draggedEventsTolerance: 3,
        inertiaDuration: 200,
        inertiaRatio: 3,
        zoomDuration: 250,
        zoomingRatio: 1.7,
        doubleClickTimeout: 300,
        doubleClickZoomingRatio: 2.2,
        doubleClickZoomingDuration: 200,
        tapMoveTolerance: 10,
        zoomToSizeRatioFunction: Math.sqrt,
        itemSizesReference: "screen",
        autoRescale: true,
        autoCenter: true,
        labelDensity: 1,
        labelGridCellSize: 100,
        labelRenderedSizeThreshold: 6,
        nodeReducer: null,
        edgeReducer: null,
        zIndex: false,
        minCameraRatio: null,
        maxCameraRatio: null,
        enableCameraZooming: true,
        enableCameraPanning: true,
        enableCameraRotation: true,
        cameraPanBoundaries: null,
        allowInvalidContainer: false,
        nodeProgramClasses: {},
        nodeHoverProgramClasses: {},
        edgeProgramClasses: {}
      }, _S = {
        circle: Oo
      }, wS = {
        arrow: JE,
        line: iS
      };
      function fc(l) {
        if (typeof l.labelDensity != "number" || l.labelDensity < 0) throw new Error("Settings: invalid `labelDensity`. Expecting a positive number.");
        var a = l.minCameraRatio, o = l.maxCameraRatio;
        if (typeof a == "number" && typeof o == "number" && o < a) throw new Error("Settings: invalid camera ratio boundaries. Expecting `maxCameraRatio` to be greater than `minCameraRatio`.");
      }
      function ES(l) {
        var a = cc({}, Ic, l);
        return a.nodeProgramClasses = cc({}, _S, a.nodeProgramClasses), a.edgeProgramClasses = cc({}, wS, a.edgeProgramClasses), a;
      }
      var bo = 1.5, Rm = function(l) {
        function a() {
          var o;
          return Dt(this, a), o = ln(this, a), ee(o, "x", 0.5), ee(o, "y", 0.5), ee(o, "angle", 0), ee(o, "ratio", 1), ee(o, "minRatio", null), ee(o, "maxRatio", null), ee(o, "enabledZooming", true), ee(o, "enabledPanning", true), ee(o, "enabledRotation", true), ee(o, "clean", null), ee(o, "nextFrame", null), ee(o, "previousState", null), ee(o, "enabled", true), o.previousState = o.getState(), o;
        }
        return on(a, l), Nt(a, [
          {
            key: "enable",
            value: function() {
              return this.enabled = true, this;
            }
          },
          {
            key: "disable",
            value: function() {
              return this.enabled = false, this;
            }
          },
          {
            key: "getState",
            value: function() {
              return {
                x: this.x,
                y: this.y,
                angle: this.angle,
                ratio: this.ratio
              };
            }
          },
          {
            key: "hasState",
            value: function(i) {
              return this.x === i.x && this.y === i.y && this.ratio === i.ratio && this.angle === i.angle;
            }
          },
          {
            key: "getPreviousState",
            value: function() {
              var i = this.previousState;
              return i ? {
                x: i.x,
                y: i.y,
                angle: i.angle,
                ratio: i.ratio
              } : null;
            }
          },
          {
            key: "getBoundedRatio",
            value: function(i) {
              var u = i;
              return typeof this.minRatio == "number" && (u = Math.max(u, this.minRatio)), typeof this.maxRatio == "number" && (u = Math.min(u, this.maxRatio)), u;
            }
          },
          {
            key: "validateState",
            value: function(i) {
              var u = {};
              return this.enabledPanning && typeof i.x == "number" && (u.x = i.x), this.enabledPanning && typeof i.y == "number" && (u.y = i.y), this.enabledZooming && typeof i.ratio == "number" && (u.ratio = this.getBoundedRatio(i.ratio)), this.enabledRotation && typeof i.angle == "number" && (u.angle = i.angle), this.clean ? this.clean(ve(ve({}, this.getState()), u)) : u;
            }
          },
          {
            key: "isAnimated",
            value: function() {
              return !!this.nextFrame;
            }
          },
          {
            key: "setState",
            value: function(i) {
              if (!this.enabled) return this;
              this.previousState = this.getState();
              var u = this.validateState(i);
              return typeof u.x == "number" && (this.x = u.x), typeof u.y == "number" && (this.y = u.y), typeof u.ratio == "number" && (this.ratio = u.ratio), typeof u.angle == "number" && (this.angle = u.angle), this.hasState(this.previousState) || this.emit("updated", this.getState()), this;
            }
          },
          {
            key: "updateState",
            value: function(i) {
              return this.setState(i(this.getState())), this;
            }
          },
          {
            key: "animate",
            value: function(i) {
              var u = this, c = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {}, d = arguments.length > 2 ? arguments[2] : void 0;
              if (!d) return new Promise(function(R) {
                return u.animate(i, c, R);
              });
              if (this.enabled) {
                var h = ve(ve({}, gS), c), m = this.validateState(i), g = typeof h.easing == "function" ? h.easing : hS[h.easing], p = Date.now(), _ = this.getState(), E = function() {
                  var z = (Date.now() - p) / h.duration;
                  if (z >= 1) {
                    u.nextFrame = null, u.setState(m), u.animationCallback && (u.animationCallback.call(null), u.animationCallback = void 0);
                    return;
                  }
                  var H = g(z), Y = {};
                  typeof m.x == "number" && (Y.x = _.x + (m.x - _.x) * H), typeof m.y == "number" && (Y.y = _.y + (m.y - _.y) * H), u.enabledRotation && typeof m.angle == "number" && (Y.angle = _.angle + (m.angle - _.angle) * H), typeof m.ratio == "number" && (Y.ratio = _.ratio + (m.ratio - _.ratio) * H), u.setState(Y), u.nextFrame = requestAnimationFrame(E);
                };
                this.nextFrame ? (cancelAnimationFrame(this.nextFrame), this.animationCallback && this.animationCallback.call(null), this.nextFrame = requestAnimationFrame(E)) : E(), this.animationCallback = d;
              }
            }
          },
          {
            key: "animatedZoom",
            value: function(i) {
              return i ? typeof i == "number" ? this.animate({
                ratio: this.ratio / i
              }) : this.animate({
                ratio: this.ratio / (i.factor || bo)
              }, i) : this.animate({
                ratio: this.ratio / bo
              });
            }
          },
          {
            key: "animatedUnzoom",
            value: function(i) {
              return i ? typeof i == "number" ? this.animate({
                ratio: this.ratio * i
              }) : this.animate({
                ratio: this.ratio * (i.factor || bo)
              }, i) : this.animate({
                ratio: this.ratio * bo
              });
            }
          },
          {
            key: "animatedReset",
            value: function(i) {
              return this.animate({
                x: 0.5,
                y: 0.5,
                ratio: 1,
                angle: 0
              }, i);
            }
          },
          {
            key: "copy",
            value: function() {
              return a.from(this.getState());
            }
          }
        ], [
          {
            key: "from",
            value: function(i) {
              var u = new a();
              return u.setState(i);
            }
          }
        ]);
      }(Pc);
      function mn(l, a) {
        var o = a.getBoundingClientRect();
        return {
          x: l.clientX - o.left,
          y: l.clientY - o.top
        };
      }
      function qn(l, a) {
        var o = ve(ve({}, mn(l, a)), {}, {
          sigmaDefaultPrevented: false,
          preventSigmaDefault: function() {
            o.sigmaDefaultPrevented = true;
          },
          original: l
        });
        return o;
      }
      function qr(l) {
        var a = "x" in l ? l : ve(ve({}, l.touches[0] || l.previousTouches[0]), {}, {
          original: l.original,
          sigmaDefaultPrevented: l.sigmaDefaultPrevented,
          preventSigmaDefault: function() {
            l.sigmaDefaultPrevented = true, a.sigmaDefaultPrevented = true;
          }
        });
        return a;
      }
      function SS(l, a) {
        return ve(ve({}, qn(l, a)), {}, {
          delta: Wv(l)
        });
      }
      var xS = 2;
      function To(l) {
        for (var a = [], o = 0, i = Math.min(l.length, xS); o < i; o++) a.push(l[o]);
        return a;
      }
      function Vr(l, a, o) {
        var i = {
          touches: To(l.touches).map(function(u) {
            return mn(u, o);
          }),
          previousTouches: a.map(function(u) {
            return mn(u, o);
          }),
          sigmaDefaultPrevented: false,
          preventSigmaDefault: function() {
            i.sigmaDefaultPrevented = true;
          },
          original: l
        };
        return i;
      }
      function Wv(l) {
        if (typeof l.deltaY < "u") return l.deltaY * -3 / 360;
        if (typeof l.detail < "u") return l.detail / -9;
        throw new Error("Captor: could not extract delta from event.");
      }
      var Jv = function(l) {
        function a(o, i) {
          var u;
          return Dt(this, a), u = ln(this, a), u.container = o, u.renderer = i, u;
        }
        return on(a, l), Nt(a);
      }(Pc), TS = [
        "doubleClickTimeout",
        "doubleClickZoomingDuration",
        "doubleClickZoomingRatio",
        "dragTimeout",
        "draggedEventsTolerance",
        "inertiaDuration",
        "inertiaRatio",
        "zoomDuration",
        "zoomingRatio"
      ], AS = TS.reduce(function(l, a) {
        return ve(ve({}, l), {}, ee({}, a, Ic[a]));
      }, {}), RS = function(l) {
        function a(o, i) {
          var u;
          return Dt(this, a), u = ln(this, a, [
            o,
            i
          ]), ee(u, "enabled", true), ee(u, "draggedEvents", 0), ee(u, "downStartTime", null), ee(u, "lastMouseX", null), ee(u, "lastMouseY", null), ee(u, "isMouseDown", false), ee(u, "isMoving", false), ee(u, "movingTimeout", null), ee(u, "startCameraState", null), ee(u, "clicks", 0), ee(u, "doubleClickTimeout", null), ee(u, "currentWheelDirection", 0), ee(u, "settings", AS), u.handleClick = u.handleClick.bind(u), u.handleRightClick = u.handleRightClick.bind(u), u.handleDown = u.handleDown.bind(u), u.handleUp = u.handleUp.bind(u), u.handleMove = u.handleMove.bind(u), u.handleWheel = u.handleWheel.bind(u), u.handleLeave = u.handleLeave.bind(u), u.handleEnter = u.handleEnter.bind(u), o.addEventListener("click", u.handleClick, {
            capture: false
          }), o.addEventListener("contextmenu", u.handleRightClick, {
            capture: false
          }), o.addEventListener("mousedown", u.handleDown, {
            capture: false
          }), o.addEventListener("wheel", u.handleWheel, {
            capture: false
          }), o.addEventListener("mouseleave", u.handleLeave, {
            capture: false
          }), o.addEventListener("mouseenter", u.handleEnter, {
            capture: false
          }), document.addEventListener("mousemove", u.handleMove, {
            capture: false
          }), document.addEventListener("mouseup", u.handleUp, {
            capture: false
          }), u;
        }
        return on(a, l), Nt(a, [
          {
            key: "kill",
            value: function() {
              var i = this.container;
              i.removeEventListener("click", this.handleClick), i.removeEventListener("contextmenu", this.handleRightClick), i.removeEventListener("mousedown", this.handleDown), i.removeEventListener("wheel", this.handleWheel), i.removeEventListener("mouseleave", this.handleLeave), i.removeEventListener("mouseenter", this.handleEnter), document.removeEventListener("mousemove", this.handleMove), document.removeEventListener("mouseup", this.handleUp);
            }
          },
          {
            key: "handleClick",
            value: function(i) {
              var u = this;
              if (this.enabled) {
                if (this.clicks++, this.clicks === 2) return this.clicks = 0, typeof this.doubleClickTimeout == "number" && (clearTimeout(this.doubleClickTimeout), this.doubleClickTimeout = null), this.handleDoubleClick(i);
                setTimeout(function() {
                  u.clicks = 0, u.doubleClickTimeout = null;
                }, this.settings.doubleClickTimeout), this.draggedEvents < this.settings.draggedEventsTolerance && this.emit("click", qn(i, this.container));
              }
            }
          },
          {
            key: "handleRightClick",
            value: function(i) {
              this.enabled && this.emit("rightClick", qn(i, this.container));
            }
          },
          {
            key: "handleDoubleClick",
            value: function(i) {
              if (this.enabled) {
                i.preventDefault(), i.stopPropagation();
                var u = qn(i, this.container);
                if (this.emit("doubleClick", u), !u.sigmaDefaultPrevented) {
                  var c = this.renderer.getCamera(), d = c.getBoundedRatio(c.getState().ratio / this.settings.doubleClickZoomingRatio);
                  c.animate(this.renderer.getViewportZoomedState(mn(i, this.container), d), {
                    easing: "quadraticInOut",
                    duration: this.settings.doubleClickZoomingDuration
                  });
                }
              }
            }
          },
          {
            key: "handleDown",
            value: function(i) {
              if (this.enabled) {
                if (i.button === 0) {
                  this.startCameraState = this.renderer.getCamera().getState();
                  var u = mn(i, this.container), c = u.x, d = u.y;
                  this.lastMouseX = c, this.lastMouseY = d, this.draggedEvents = 0, this.downStartTime = Date.now(), this.isMouseDown = true;
                }
                this.emit("mousedown", qn(i, this.container));
              }
            }
          },
          {
            key: "handleUp",
            value: function(i) {
              var u = this;
              if (!(!this.enabled || !this.isMouseDown)) {
                var c = this.renderer.getCamera();
                this.isMouseDown = false, typeof this.movingTimeout == "number" && (clearTimeout(this.movingTimeout), this.movingTimeout = null);
                var d = mn(i, this.container), h = d.x, m = d.y, g = c.getState(), p = c.getPreviousState() || {
                  x: 0,
                  y: 0
                };
                this.isMoving ? c.animate({
                  x: g.x + this.settings.inertiaRatio * (g.x - p.x),
                  y: g.y + this.settings.inertiaRatio * (g.y - p.y)
                }, {
                  duration: this.settings.inertiaDuration,
                  easing: "quadraticOut"
                }) : (this.lastMouseX !== h || this.lastMouseY !== m) && c.setState({
                  x: g.x,
                  y: g.y
                }), this.isMoving = false, setTimeout(function() {
                  var _ = u.draggedEvents > 0;
                  u.draggedEvents = 0, _ && u.renderer.getSetting("hideEdgesOnMove") && u.renderer.refresh();
                }, 0), this.emit("mouseup", qn(i, this.container));
              }
            }
          },
          {
            key: "handleMove",
            value: function(i) {
              var u = this;
              if (this.enabled) {
                var c = qn(i, this.container);
                if (this.emit("mousemovebody", c), (i.target === this.container || i.composedPath()[0] === this.container) && this.emit("mousemove", c), !c.sigmaDefaultPrevented && this.isMouseDown) {
                  this.isMoving = true, this.draggedEvents++, typeof this.movingTimeout == "number" && clearTimeout(this.movingTimeout), this.movingTimeout = window.setTimeout(function() {
                    u.movingTimeout = null, u.isMoving = false;
                  }, this.settings.dragTimeout);
                  var d = this.renderer.getCamera(), h = mn(i, this.container), m = h.x, g = h.y, p = this.renderer.viewportToFramedGraph({
                    x: this.lastMouseX,
                    y: this.lastMouseY
                  }), _ = this.renderer.viewportToFramedGraph({
                    x: m,
                    y: g
                  }), E = p.x - _.x, R = p.y - _.y, z = d.getState(), H = z.x + E, Y = z.y + R;
                  d.setState({
                    x: H,
                    y: Y
                  }), this.lastMouseX = m, this.lastMouseY = g, i.preventDefault(), i.stopPropagation();
                }
              }
            }
          },
          {
            key: "handleLeave",
            value: function(i) {
              this.emit("mouseleave", qn(i, this.container));
            }
          },
          {
            key: "handleEnter",
            value: function(i) {
              this.emit("mouseenter", qn(i, this.container));
            }
          },
          {
            key: "handleWheel",
            value: function(i) {
              var u = this, c = this.renderer.getCamera();
              if (!(!this.enabled || !c.enabledZooming)) {
                var d = Wv(i);
                if (d) {
                  var h = SS(i, this.container);
                  if (this.emit("wheel", h), h.sigmaDefaultPrevented) {
                    i.preventDefault(), i.stopPropagation();
                    return;
                  }
                  var m = c.getState().ratio, g = d > 0 ? 1 / this.settings.zoomingRatio : this.settings.zoomingRatio, p = c.getBoundedRatio(m * g), _ = d > 0 ? 1 : -1, E = Date.now();
                  m !== p && (i.preventDefault(), i.stopPropagation(), !(this.currentWheelDirection === _ && this.lastWheelTriggerTime && E - this.lastWheelTriggerTime < this.settings.zoomDuration / 5) && (c.animate(this.renderer.getViewportZoomedState(mn(i, this.container), p), {
                    easing: "quadraticOut",
                    duration: this.settings.zoomDuration
                  }, function() {
                    u.currentWheelDirection = 0;
                  }), this.currentWheelDirection = _, this.lastWheelTriggerTime = E));
                }
              }
            }
          },
          {
            key: "setSettings",
            value: function(i) {
              this.settings = i;
            }
          }
        ]);
      }(Jv), CS = [
        "dragTimeout",
        "inertiaDuration",
        "inertiaRatio",
        "doubleClickTimeout",
        "doubleClickZoomingRatio",
        "doubleClickZoomingDuration",
        "tapMoveTolerance"
      ], DS = CS.reduce(function(l, a) {
        return ve(ve({}, l), {}, ee({}, a, Ic[a]));
      }, {}), NS = function(l) {
        function a(o, i) {
          var u;
          return Dt(this, a), u = ln(this, a, [
            o,
            i
          ]), ee(u, "enabled", true), ee(u, "isMoving", false), ee(u, "hasMoved", false), ee(u, "touchMode", 0), ee(u, "startTouchesPositions", []), ee(u, "lastTouches", []), ee(u, "lastTap", null), ee(u, "settings", DS), u.handleStart = u.handleStart.bind(u), u.handleLeave = u.handleLeave.bind(u), u.handleMove = u.handleMove.bind(u), o.addEventListener("touchstart", u.handleStart, {
            capture: false
          }), o.addEventListener("touchcancel", u.handleLeave, {
            capture: false
          }), document.addEventListener("touchend", u.handleLeave, {
            capture: false,
            passive: false
          }), document.addEventListener("touchmove", u.handleMove, {
            capture: false,
            passive: false
          }), u;
        }
        return on(a, l), Nt(a, [
          {
            key: "kill",
            value: function() {
              var i = this.container;
              i.removeEventListener("touchstart", this.handleStart), i.removeEventListener("touchcancel", this.handleLeave), document.removeEventListener("touchend", this.handleLeave), document.removeEventListener("touchmove", this.handleMove);
            }
          },
          {
            key: "getDimensions",
            value: function() {
              return {
                width: this.container.offsetWidth,
                height: this.container.offsetHeight
              };
            }
          },
          {
            key: "handleStart",
            value: function(i) {
              var u = this;
              if (this.enabled) {
                i.preventDefault();
                var c = To(i.touches);
                if (this.touchMode = c.length, this.startCameraState = this.renderer.getCamera().getState(), this.startTouchesPositions = c.map(function(R) {
                  return mn(R, u.container);
                }), this.touchMode === 2) {
                  var d = ka(this.startTouchesPositions, 2), h = d[0], m = h.x, g = h.y, p = d[1], _ = p.x, E = p.y;
                  this.startTouchesAngle = Math.atan2(E - g, _ - m), this.startTouchesDistance = Math.sqrt(Math.pow(_ - m, 2) + Math.pow(E - g, 2));
                }
                this.emit("touchdown", Vr(i, this.lastTouches, this.container)), this.lastTouches = c, this.lastTouchesPositions = this.startTouchesPositions;
              }
            }
          },
          {
            key: "handleLeave",
            value: function(i) {
              if (!(!this.enabled || !this.startTouchesPositions.length)) {
                switch (i.cancelable && i.preventDefault(), this.movingTimeout && (this.isMoving = false, clearTimeout(this.movingTimeout)), this.touchMode) {
                  case 2:
                    if (i.touches.length === 1) {
                      this.handleStart(i), i.preventDefault();
                      break;
                    }
                  case 1:
                    if (this.isMoving) {
                      var u = this.renderer.getCamera(), c = u.getState(), d = u.getPreviousState() || {
                        x: 0,
                        y: 0
                      };
                      u.animate({
                        x: c.x + this.settings.inertiaRatio * (c.x - d.x),
                        y: c.y + this.settings.inertiaRatio * (c.y - d.y)
                      }, {
                        duration: this.settings.inertiaDuration,
                        easing: "quadraticOut"
                      });
                    }
                    this.hasMoved = false, this.isMoving = false, this.touchMode = 0;
                    break;
                }
                if (this.emit("touchup", Vr(i, this.lastTouches, this.container)), !i.touches.length) {
                  var h = mn(this.lastTouches[0], this.container), m = this.startTouchesPositions[0], g = Math.pow(h.x - m.x, 2) + Math.pow(h.y - m.y, 2);
                  if (!i.touches.length && g < Math.pow(this.settings.tapMoveTolerance, 2)) if (this.lastTap && Date.now() - this.lastTap.time < this.settings.doubleClickTimeout) {
                    var p = Vr(i, this.lastTouches, this.container);
                    if (this.emit("doubletap", p), this.lastTap = null, !p.sigmaDefaultPrevented) {
                      var _ = this.renderer.getCamera(), E = _.getBoundedRatio(_.getState().ratio / this.settings.doubleClickZoomingRatio);
                      _.animate(this.renderer.getViewportZoomedState(h, E), {
                        easing: "quadraticInOut",
                        duration: this.settings.doubleClickZoomingDuration
                      });
                    }
                  } else {
                    var R = Vr(i, this.lastTouches, this.container);
                    this.emit("tap", R), this.lastTap = {
                      time: Date.now(),
                      position: R.touches[0] || R.previousTouches[0]
                    };
                  }
                }
                this.lastTouches = To(i.touches), this.startTouchesPositions = [];
              }
            }
          },
          {
            key: "handleMove",
            value: function(i) {
              var u = this;
              if (!(!this.enabled || !this.startTouchesPositions.length)) {
                i.preventDefault();
                var c = To(i.touches), d = c.map(function(k) {
                  return mn(k, u.container);
                }), h = this.lastTouches;
                this.lastTouches = c, this.lastTouchesPositions = d;
                var m = Vr(i, h, this.container);
                if (this.emit("touchmove", m), !m.sigmaDefaultPrevented && (this.hasMoved || (this.hasMoved = d.some(function(k, K) {
                  var U = u.startTouchesPositions[K];
                  return U && (k.x !== U.x || k.y !== U.y);
                })), !!this.hasMoved)) {
                  this.isMoving = true, this.movingTimeout && clearTimeout(this.movingTimeout), this.movingTimeout = window.setTimeout(function() {
                    u.isMoving = false;
                  }, this.settings.dragTimeout);
                  var g = this.renderer.getCamera(), p = this.startCameraState, _ = this.renderer.getSetting("stagePadding");
                  switch (this.touchMode) {
                    case 1: {
                      var E = this.renderer.viewportToFramedGraph((this.startTouchesPositions || [])[0]), R = E.x, z = E.y, H = this.renderer.viewportToFramedGraph(d[0]), Y = H.x, te = H.y;
                      g.setState({
                        x: p.x + R - Y,
                        y: p.y + z - te
                      });
                      break;
                    }
                    case 2: {
                      var J = {
                        x: 0.5,
                        y: 0.5,
                        angle: 0,
                        ratio: 1
                      }, I = d[0], G = I.x, B = I.y, Z = d[1], T = Z.x, y = Z.y, w = Math.atan2(y - B, T - G) - this.startTouchesAngle, S = Math.hypot(y - B, T - G) / this.startTouchesDistance, N = g.getBoundedRatio(p.ratio / S);
                      J.ratio = N, J.angle = p.angle + w;
                      var L = this.getDimensions(), re = this.renderer.viewportToFramedGraph((this.startTouchesPositions || [])[0], {
                        cameraState: p
                      }), ue = Math.min(L.width, L.height) - 2 * _, oe = ue / L.width, D = ue / L.height, X = N / ue, $ = G - ue / 2 / oe, ne = B - ue / 2 / D, x = [
                        $ * Math.cos(-J.angle) - ne * Math.sin(-J.angle),
                        ne * Math.cos(-J.angle) + $ * Math.sin(-J.angle)
                      ];
                      $ = x[0], ne = x[1], J.x = re.x - $ * X, J.y = re.y + ne * X, g.setState(J);
                      break;
                    }
                  }
                }
              }
            }
          },
          {
            key: "setSettings",
            value: function(i) {
              this.settings = i;
            }
          }
        ]);
      }(Jv);
      function OS(l) {
        if (Array.isArray(l)) return Cc(l);
      }
      function zS(l) {
        if (typeof Symbol < "u" && l[Symbol.iterator] != null || l["@@iterator"] != null) return Array.from(l);
      }
      function kS() {
        throw new TypeError(`Invalid attempt to spread non-iterable instance.
In order to be iterable, non-array objects must have a [Symbol.iterator]() method.`);
      }
      function Cm(l) {
        return OS(l) || zS(l) || kv(l) || kS();
      }
      function MS(l, a) {
        if (l == null) return {};
        var o = {};
        for (var i in l) if ({}.hasOwnProperty.call(l, i)) {
          if (a.indexOf(i) !== -1) continue;
          o[i] = l[i];
        }
        return o;
      }
      function dc(l, a) {
        if (l == null) return {};
        var o, i, u = MS(l, a);
        if (Object.getOwnPropertySymbols) {
          var c = Object.getOwnPropertySymbols(l);
          for (i = 0; i < c.length; i++) o = c[i], a.indexOf(o) === -1 && {}.propertyIsEnumerable.call(l, o) && (u[o] = l[o]);
        }
        return u;
      }
      var Dm = function() {
        function l(a, o) {
          Dt(this, l), this.key = a, this.size = o;
        }
        return Nt(l, null, [
          {
            key: "compare",
            value: function(o, i) {
              return o.size > i.size ? -1 : o.size < i.size || o.key > i.key ? 1 : -1;
            }
          }
        ]);
      }(), Nm = function() {
        function l() {
          Dt(this, l), ee(this, "width", 0), ee(this, "height", 0), ee(this, "cellSize", 0), ee(this, "columns", 0), ee(this, "rows", 0), ee(this, "cells", {});
        }
        return Nt(l, [
          {
            key: "resizeAndClear",
            value: function(o, i) {
              this.width = o.width, this.height = o.height, this.cellSize = i, this.columns = Math.ceil(o.width / i), this.rows = Math.ceil(o.height / i), this.cells = {};
            }
          },
          {
            key: "getIndex",
            value: function(o) {
              var i = Math.floor(o.x / this.cellSize), u = Math.floor(o.y / this.cellSize);
              return u * this.columns + i;
            }
          },
          {
            key: "add",
            value: function(o, i, u) {
              var c = new Dm(o, i), d = this.getIndex(u), h = this.cells[d];
              h || (h = [], this.cells[d] = h), h.push(c);
            }
          },
          {
            key: "organize",
            value: function() {
              for (var o in this.cells) {
                var i = this.cells[o];
                i.sort(Dm.compare);
              }
            }
          },
          {
            key: "getLabelsToDisplay",
            value: function(o, i) {
              var u = this.cellSize * this.cellSize, c = u / o / o, d = c * i / u, h = Math.ceil(d), m = [];
              for (var g in this.cells) for (var p = this.cells[g], _ = 0; _ < Math.min(h, p.length); _++) m.push(p[_].key);
              return m;
            }
          }
        ]);
      }();
      function LS(l) {
        var a = l.graph, o = l.hoveredNode, i = l.highlightedNodes, u = l.displayedNodeLabels, c = [];
        return a.forEachEdge(function(d, h, m, g) {
          (m === o || g === o || i.has(m) || i.has(g) || u.has(m) && u.has(g)) && c.push(d);
        }), c;
      }
      var GS = 150, US = 50, Vn = Object.prototype.hasOwnProperty;
      function jS(l, a, o) {
        if (!Vn.call(o, "x") || !Vn.call(o, "y")) throw new Error('Sigma: could not find a valid position (x, y) for node "'.concat(a, '". All your nodes must have a number "x" and "y". Maybe your forgot to apply a layout or your "nodeReducer" is not returning the correct data?'));
        return o.color || (o.color = l.defaultNodeColor), !o.label && o.label !== "" && (o.label = null), o.label !== void 0 && o.label !== null ? o.label = "" + o.label : o.label = null, o.size || (o.size = 2), Vn.call(o, "hidden") || (o.hidden = false), Vn.call(o, "highlighted") || (o.highlighted = false), Vn.call(o, "forceLabel") || (o.forceLabel = false), (!o.type || o.type === "") && (o.type = l.defaultNodeType), o.zIndex || (o.zIndex = 0), o;
      }
      function BS(l, a, o) {
        return o.color || (o.color = l.defaultEdgeColor), o.label || (o.label = ""), o.size || (o.size = 0.5), Vn.call(o, "hidden") || (o.hidden = false), Vn.call(o, "forceLabel") || (o.forceLabel = false), (!o.type || o.type === "") && (o.type = l.defaultEdgeType), o.zIndex || (o.zIndex = 0), o;
      }
      var HS = function(l) {
        function a(o, i) {
          var u, c = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : {};
          if (Dt(this, a), u = ln(this, a), ee(u, "elements", {}), ee(u, "canvasContexts", {}), ee(u, "webGLContexts", {}), ee(u, "pickingLayers", /* @__PURE__ */ new Set()), ee(u, "textures", {}), ee(u, "frameBuffers", {}), ee(u, "activeListeners", {}), ee(u, "labelGrid", new Nm()), ee(u, "nodeDataCache", {}), ee(u, "edgeDataCache", {}), ee(u, "nodeProgramIndex", {}), ee(u, "edgeProgramIndex", {}), ee(u, "nodesWithForcedLabels", /* @__PURE__ */ new Set()), ee(u, "edgesWithForcedLabels", /* @__PURE__ */ new Set()), ee(u, "nodeExtent", {
            x: [
              0,
              1
            ],
            y: [
              0,
              1
            ]
          }), ee(u, "nodeZExtent", [
            1 / 0,
            -1 / 0
          ]), ee(u, "edgeZExtent", [
            1 / 0,
            -1 / 0
          ]), ee(u, "matrix", gn()), ee(u, "invMatrix", gn()), ee(u, "correctionRatio", 1), ee(u, "customBBox", null), ee(u, "normalizationFunction", Tm({
            x: [
              0,
              1
            ],
            y: [
              0,
              1
            ]
          })), ee(u, "graphToViewportRatio", 1), ee(u, "itemIDsIndex", {}), ee(u, "nodeIndices", {}), ee(u, "edgeIndices", {}), ee(u, "width", 0), ee(u, "height", 0), ee(u, "pixelRatio", Sm()), ee(u, "pickingDownSizingRatio", 2 * u.pixelRatio), ee(u, "displayedNodeLabels", /* @__PURE__ */ new Set()), ee(u, "displayedEdgeLabels", /* @__PURE__ */ new Set()), ee(u, "highlightedNodes", /* @__PURE__ */ new Set()), ee(u, "hoveredNode", null), ee(u, "hoveredEdge", null), ee(u, "renderFrame", null), ee(u, "renderHighlightedNodesFrame", null), ee(u, "needToProcess", false), ee(u, "checkEdgesEventsFrame", null), ee(u, "nodePrograms", {}), ee(u, "nodeHoverPrograms", {}), ee(u, "edgePrograms", {}), u.settings = ES(c), fc(u.settings), yS(o), !(i instanceof HTMLElement)) throw new Error("Sigma: container should be an html element.");
          u.graph = o, u.container = i, u.createWebGLContext("edges", {
            picking: c.enableEdgeEvents
          }), u.createCanvasContext("edgeLabels"), u.createWebGLContext("nodes", {
            picking: true
          }), u.createCanvasContext("labels"), u.createCanvasContext("hovers"), u.createWebGLContext("hoverNodes"), u.createCanvasContext("mouse", {
            style: {
              touchAction: "none",
              userSelect: "none"
            }
          }), u.resize();
          for (var d in u.settings.nodeProgramClasses) u.registerNodeProgram(d, u.settings.nodeProgramClasses[d], u.settings.nodeHoverProgramClasses[d]);
          for (var h in u.settings.edgeProgramClasses) u.registerEdgeProgram(h, u.settings.edgeProgramClasses[h]);
          return u.camera = new Rm(), u.bindCameraHandlers(), u.mouseCaptor = new RS(u.elements.mouse, u), u.mouseCaptor.setSettings(u.settings), u.touchCaptor = new NS(u.elements.mouse, u), u.touchCaptor.setSettings(u.settings), u.bindEventHandlers(), u.bindGraphHandlers(), u.handleSettingsUpdate(), u.refresh(), u;
        }
        return on(a, l), Nt(a, [
          {
            key: "registerNodeProgram",
            value: function(i, u, c) {
              return this.nodePrograms[i] && this.nodePrograms[i].kill(), this.nodeHoverPrograms[i] && this.nodeHoverPrograms[i].kill(), this.nodePrograms[i] = new u(this.webGLContexts.nodes, this.frameBuffers.nodes, this), this.nodeHoverPrograms[i] = new (c || u)(this.webGLContexts.hoverNodes, null, this), this;
            }
          },
          {
            key: "registerEdgeProgram",
            value: function(i, u) {
              return this.edgePrograms[i] && this.edgePrograms[i].kill(), this.edgePrograms[i] = new u(this.webGLContexts.edges, this.frameBuffers.edges, this), this;
            }
          },
          {
            key: "unregisterNodeProgram",
            value: function(i) {
              if (this.nodePrograms[i]) {
                var u = this.nodePrograms, c = u[i], d = dc(u, [
                  i
                ].map(Yr));
                c.kill(), this.nodePrograms = d;
              }
              if (this.nodeHoverPrograms[i]) {
                var h = this.nodeHoverPrograms, m = h[i], g = dc(h, [
                  i
                ].map(Yr));
                m.kill(), this.nodePrograms = g;
              }
              return this;
            }
          },
          {
            key: "unregisterEdgeProgram",
            value: function(i) {
              if (this.edgePrograms[i]) {
                var u = this.edgePrograms, c = u[i], d = dc(u, [
                  i
                ].map(Yr));
                c.kill(), this.edgePrograms = d;
              }
              return this;
            }
          },
          {
            key: "resetWebGLTexture",
            value: function(i) {
              var u = this.webGLContexts[i], c = this.frameBuffers[i], d = this.textures[i];
              d && u.deleteTexture(d);
              var h = u.createTexture();
              return u.bindFramebuffer(u.FRAMEBUFFER, c), u.bindTexture(u.TEXTURE_2D, h), u.texImage2D(u.TEXTURE_2D, 0, u.RGBA, this.width, this.height, 0, u.RGBA, u.UNSIGNED_BYTE, null), u.framebufferTexture2D(u.FRAMEBUFFER, u.COLOR_ATTACHMENT0, u.TEXTURE_2D, h, 0), this.textures[i] = h, this;
            }
          },
          {
            key: "bindCameraHandlers",
            value: function() {
              var i = this;
              return this.activeListeners.camera = function() {
                i.scheduleRender();
              }, this.camera.on("updated", this.activeListeners.camera), this;
            }
          },
          {
            key: "unbindCameraHandlers",
            value: function() {
              return this.camera.removeListener("updated", this.activeListeners.camera), this;
            }
          },
          {
            key: "getNodeAtPosition",
            value: function(i) {
              var u = i.x, c = i.y, d = fm(this.webGLContexts.nodes, this.frameBuffers.nodes, u, c, this.pixelRatio, this.pickingDownSizingRatio), h = cm.apply(void 0, Cm(d)), m = this.itemIDsIndex[h];
              return m && m.type === "node" ? m.id : null;
            }
          },
          {
            key: "bindEventHandlers",
            value: function() {
              var i = this;
              this.activeListeners.handleResize = function() {
                i.scheduleRefresh();
              }, window.addEventListener("resize", this.activeListeners.handleResize), this.activeListeners.handleMove = function(c) {
                var d = qr(c), h = {
                  event: d,
                  preventSigmaDefault: function() {
                    d.preventSigmaDefault();
                  }
                }, m = i.getNodeAtPosition(d);
                if (m && i.hoveredNode !== m && !i.nodeDataCache[m].hidden) {
                  i.hoveredNode && i.emit("leaveNode", ve(ve({}, h), {}, {
                    node: i.hoveredNode
                  })), i.hoveredNode = m, i.emit("enterNode", ve(ve({}, h), {}, {
                    node: m
                  })), i.scheduleHighlightedNodesRender();
                  return;
                }
                if (i.hoveredNode && i.getNodeAtPosition(d) !== i.hoveredNode) {
                  var g = i.hoveredNode;
                  i.hoveredNode = null, i.emit("leaveNode", ve(ve({}, h), {}, {
                    node: g
                  })), i.scheduleHighlightedNodesRender();
                  return;
                }
                if (i.settings.enableEdgeEvents) {
                  var p = i.hoveredNode ? null : i.getEdgeAtPoint(h.event.x, h.event.y);
                  p !== i.hoveredEdge && (i.hoveredEdge && i.emit("leaveEdge", ve(ve({}, h), {}, {
                    edge: i.hoveredEdge
                  })), p && i.emit("enterEdge", ve(ve({}, h), {}, {
                    edge: p
                  })), i.hoveredEdge = p);
                }
              }, this.activeListeners.handleMoveBody = function(c) {
                var d = qr(c);
                i.emit("moveBody", {
                  event: d,
                  preventSigmaDefault: function() {
                    d.preventSigmaDefault();
                  }
                });
              }, this.activeListeners.handleLeave = function(c) {
                var d = qr(c), h = {
                  event: d,
                  preventSigmaDefault: function() {
                    d.preventSigmaDefault();
                  }
                };
                i.hoveredNode && (i.emit("leaveNode", ve(ve({}, h), {}, {
                  node: i.hoveredNode
                })), i.scheduleHighlightedNodesRender()), i.settings.enableEdgeEvents && i.hoveredEdge && (i.emit("leaveEdge", ve(ve({}, h), {}, {
                  edge: i.hoveredEdge
                })), i.scheduleHighlightedNodesRender()), i.emit("leaveStage", ve({}, h));
              }, this.activeListeners.handleEnter = function(c) {
                var d = qr(c), h = {
                  event: d,
                  preventSigmaDefault: function() {
                    d.preventSigmaDefault();
                  }
                };
                i.emit("enterStage", ve({}, h));
              };
              var u = function(d) {
                return function(h) {
                  var m = qr(h), g = {
                    event: m,
                    preventSigmaDefault: function() {
                      m.preventSigmaDefault();
                    }
                  }, p = i.getNodeAtPosition(m);
                  if (p) return i.emit("".concat(d, "Node"), ve(ve({}, g), {}, {
                    node: p
                  }));
                  if (i.settings.enableEdgeEvents) {
                    var _ = i.getEdgeAtPoint(m.x, m.y);
                    if (_) return i.emit("".concat(d, "Edge"), ve(ve({}, g), {}, {
                      edge: _
                    }));
                  }
                  return i.emit("".concat(d, "Stage"), g);
                };
              };
              return this.activeListeners.handleClick = u("click"), this.activeListeners.handleRightClick = u("rightClick"), this.activeListeners.handleDoubleClick = u("doubleClick"), this.activeListeners.handleWheel = u("wheel"), this.activeListeners.handleDown = u("down"), this.activeListeners.handleUp = u("up"), this.mouseCaptor.on("mousemove", this.activeListeners.handleMove), this.mouseCaptor.on("mousemovebody", this.activeListeners.handleMoveBody), this.mouseCaptor.on("click", this.activeListeners.handleClick), this.mouseCaptor.on("rightClick", this.activeListeners.handleRightClick), this.mouseCaptor.on("doubleClick", this.activeListeners.handleDoubleClick), this.mouseCaptor.on("wheel", this.activeListeners.handleWheel), this.mouseCaptor.on("mousedown", this.activeListeners.handleDown), this.mouseCaptor.on("mouseup", this.activeListeners.handleUp), this.mouseCaptor.on("mouseleave", this.activeListeners.handleLeave), this.mouseCaptor.on("mouseenter", this.activeListeners.handleEnter), this.touchCaptor.on("touchdown", this.activeListeners.handleDown), this.touchCaptor.on("touchdown", this.activeListeners.handleMove), this.touchCaptor.on("touchup", this.activeListeners.handleUp), this.touchCaptor.on("touchmove", this.activeListeners.handleMove), this.touchCaptor.on("tap", this.activeListeners.handleClick), this.touchCaptor.on("doubletap", this.activeListeners.handleDoubleClick), this.touchCaptor.on("touchmove", this.activeListeners.handleMoveBody), this;
            }
          },
          {
            key: "bindGraphHandlers",
            value: function() {
              var i = this, u = this.graph, c = /* @__PURE__ */ new Set([
                "x",
                "y",
                "zIndex",
                "type"
              ]);
              return this.activeListeners.eachNodeAttributesUpdatedGraphUpdate = function(d) {
                var h, m = (h = d.hints) === null || h === void 0 ? void 0 : h.attributes;
                i.graph.forEachNode(function(p) {
                  return i.updateNode(p);
                });
                var g = !m || m.some(function(p) {
                  return c.has(p);
                });
                i.refresh({
                  partialGraph: {
                    nodes: u.nodes()
                  },
                  skipIndexation: !g,
                  schedule: true
                });
              }, this.activeListeners.eachEdgeAttributesUpdatedGraphUpdate = function(d) {
                var h, m = (h = d.hints) === null || h === void 0 ? void 0 : h.attributes;
                i.graph.forEachEdge(function(p) {
                  return i.updateEdge(p);
                });
                var g = m && [
                  "zIndex",
                  "type"
                ].some(function(p) {
                  return m == null ? void 0 : m.includes(p);
                });
                i.refresh({
                  partialGraph: {
                    edges: u.edges()
                  },
                  skipIndexation: !g,
                  schedule: true
                });
              }, this.activeListeners.addNodeGraphUpdate = function(d) {
                var h = d.key;
                i.addNode(h), i.refresh({
                  partialGraph: {
                    nodes: [
                      h
                    ]
                  },
                  skipIndexation: false,
                  schedule: true
                });
              }, this.activeListeners.updateNodeGraphUpdate = function(d) {
                var h = d.key;
                i.refresh({
                  partialGraph: {
                    nodes: [
                      h
                    ]
                  },
                  skipIndexation: false,
                  schedule: true
                });
              }, this.activeListeners.dropNodeGraphUpdate = function(d) {
                var h = d.key;
                i.removeNode(h), i.refresh({
                  schedule: true
                });
              }, this.activeListeners.addEdgeGraphUpdate = function(d) {
                var h = d.key;
                i.addEdge(h), i.refresh({
                  partialGraph: {
                    edges: [
                      h
                    ]
                  },
                  schedule: true
                });
              }, this.activeListeners.updateEdgeGraphUpdate = function(d) {
                var h = d.key;
                i.refresh({
                  partialGraph: {
                    edges: [
                      h
                    ]
                  },
                  skipIndexation: false,
                  schedule: true
                });
              }, this.activeListeners.dropEdgeGraphUpdate = function(d) {
                var h = d.key;
                i.removeEdge(h), i.refresh({
                  schedule: true
                });
              }, this.activeListeners.clearEdgesGraphUpdate = function() {
                i.clearEdgeState(), i.clearEdgeIndices(), i.refresh({
                  schedule: true
                });
              }, this.activeListeners.clearGraphUpdate = function() {
                i.clearEdgeState(), i.clearNodeState(), i.clearEdgeIndices(), i.clearNodeIndices(), i.refresh({
                  schedule: true
                });
              }, u.on("nodeAdded", this.activeListeners.addNodeGraphUpdate), u.on("nodeDropped", this.activeListeners.dropNodeGraphUpdate), u.on("nodeAttributesUpdated", this.activeListeners.updateNodeGraphUpdate), u.on("eachNodeAttributesUpdated", this.activeListeners.eachNodeAttributesUpdatedGraphUpdate), u.on("edgeAdded", this.activeListeners.addEdgeGraphUpdate), u.on("edgeDropped", this.activeListeners.dropEdgeGraphUpdate), u.on("edgeAttributesUpdated", this.activeListeners.updateEdgeGraphUpdate), u.on("eachEdgeAttributesUpdated", this.activeListeners.eachEdgeAttributesUpdatedGraphUpdate), u.on("edgesCleared", this.activeListeners.clearEdgesGraphUpdate), u.on("cleared", this.activeListeners.clearGraphUpdate), this;
            }
          },
          {
            key: "unbindGraphHandlers",
            value: function() {
              var i = this.graph;
              i.removeListener("nodeAdded", this.activeListeners.addNodeGraphUpdate), i.removeListener("nodeDropped", this.activeListeners.dropNodeGraphUpdate), i.removeListener("nodeAttributesUpdated", this.activeListeners.updateNodeGraphUpdate), i.removeListener("eachNodeAttributesUpdated", this.activeListeners.eachNodeAttributesUpdatedGraphUpdate), i.removeListener("edgeAdded", this.activeListeners.addEdgeGraphUpdate), i.removeListener("edgeDropped", this.activeListeners.dropEdgeGraphUpdate), i.removeListener("edgeAttributesUpdated", this.activeListeners.updateEdgeGraphUpdate), i.removeListener("eachEdgeAttributesUpdated", this.activeListeners.eachEdgeAttributesUpdatedGraphUpdate), i.removeListener("edgesCleared", this.activeListeners.clearEdgesGraphUpdate), i.removeListener("cleared", this.activeListeners.clearGraphUpdate);
            }
          },
          {
            key: "getEdgeAtPoint",
            value: function(i, u) {
              var c = fm(this.webGLContexts.edges, this.frameBuffers.edges, i, u, this.pixelRatio, this.pickingDownSizingRatio), d = cm.apply(void 0, Cm(c)), h = this.itemIDsIndex[d];
              return h && h.type === "edge" ? h.id : null;
            }
          },
          {
            key: "process",
            value: function() {
              var i = this;
              this.emit("beforeProcess");
              var u = this.graph, c = this.settings, d = this.getDimensions();
              if (this.nodeExtent = pS(this.graph), !this.settings.autoRescale) {
                var h = d.width, m = d.height, g = this.nodeExtent, p = g.x, _ = g.y;
                this.nodeExtent = {
                  x: [
                    (p[0] + p[1]) / 2 - h / 2,
                    (p[0] + p[1]) / 2 + h / 2
                  ],
                  y: [
                    (_[0] + _[1]) / 2 - m / 2,
                    (_[0] + _[1]) / 2 + m / 2
                  ]
                };
              }
              this.normalizationFunction = Tm(this.customBBox || this.nodeExtent);
              var E = new Rm(), R = Fr(E.getState(), d, this.getGraphDimensions(), this.getStagePadding());
              this.labelGrid.resizeAndClear(d, c.labelGridCellSize);
              for (var z = {}, H = {}, Y = {}, te = {}, J = 1, I = u.nodes(), G = 0, B = I.length; G < B; G++) {
                var Z = I[G], T = this.nodeDataCache[Z], y = u.getNodeAttributes(Z);
                T.x = y.x, T.y = y.y, this.normalizationFunction.applyTo(T), typeof T.label == "string" && !T.hidden && this.labelGrid.add(Z, T.size, this.framedGraphToViewport(T, {
                  matrix: R
                })), z[T.type] = (z[T.type] || 0) + 1;
              }
              this.labelGrid.organize();
              for (var w in this.nodePrograms) {
                if (!Vn.call(this.nodePrograms, w)) throw new Error('Sigma: could not find a suitable program for node type "'.concat(w, '"!'));
                this.nodePrograms[w].reallocate(z[w] || 0), z[w] = 0;
              }
              this.settings.zIndex && this.nodeZExtent[0] !== this.nodeZExtent[1] && (I = xm(this.nodeZExtent, function(de) {
                return i.nodeDataCache[de].zIndex;
              }, I));
              for (var S = 0, N = I.length; S < N; S++) {
                var L = I[S];
                H[L] = J, te[H[L]] = {
                  type: "node",
                  id: L
                }, J++;
                var re = this.nodeDataCache[L];
                this.addNodeToProgram(L, H[L], z[re.type]++);
              }
              for (var ue = {}, oe = u.edges(), D = 0, X = oe.length; D < X; D++) {
                var $ = oe[D], ne = this.edgeDataCache[$];
                ue[ne.type] = (ue[ne.type] || 0) + 1;
              }
              this.settings.zIndex && this.edgeZExtent[0] !== this.edgeZExtent[1] && (oe = xm(this.edgeZExtent, function(de) {
                return i.edgeDataCache[de].zIndex;
              }, oe));
              for (var x in this.edgePrograms) {
                if (!Vn.call(this.edgePrograms, x)) throw new Error('Sigma: could not find a suitable program for edge type "'.concat(x, '"!'));
                this.edgePrograms[x].reallocate(ue[x] || 0), ue[x] = 0;
              }
              for (var k = 0, K = oe.length; k < K; k++) {
                var U = oe[k];
                Y[U] = J, te[Y[U]] = {
                  type: "edge",
                  id: U
                }, J++;
                var ie = this.edgeDataCache[U];
                this.addEdgeToProgram(U, Y[U], ue[ie.type]++);
              }
              return this.itemIDsIndex = te, this.nodeIndices = H, this.edgeIndices = Y, this.emit("afterProcess"), this;
            }
          },
          {
            key: "handleSettingsUpdate",
            value: function(i) {
              var u = this, c = this.settings;
              if (this.camera.minRatio = c.minCameraRatio, this.camera.maxRatio = c.maxCameraRatio, this.camera.enabledZooming = c.enableCameraZooming, this.camera.enabledPanning = c.enableCameraPanning, this.camera.enabledRotation = c.enableCameraRotation, c.cameraPanBoundaries ? this.camera.clean = function(p) {
                return u.cleanCameraState(p, c.cameraPanBoundaries && Oc(c.cameraPanBoundaries) === "object" ? c.cameraPanBoundaries : {});
              } : this.camera.clean = null, this.camera.setState(this.camera.validateState(this.camera.getState())), i) {
                if (i.edgeProgramClasses !== c.edgeProgramClasses) {
                  for (var d in c.edgeProgramClasses) c.edgeProgramClasses[d] !== i.edgeProgramClasses[d] && this.registerEdgeProgram(d, c.edgeProgramClasses[d]);
                  for (var h in i.edgeProgramClasses) c.edgeProgramClasses[h] || this.unregisterEdgeProgram(h);
                }
                if (i.nodeProgramClasses !== c.nodeProgramClasses || i.nodeHoverProgramClasses !== c.nodeHoverProgramClasses) {
                  for (var m in c.nodeProgramClasses) (c.nodeProgramClasses[m] !== i.nodeProgramClasses[m] || c.nodeHoverProgramClasses[m] !== i.nodeHoverProgramClasses[m]) && this.registerNodeProgram(m, c.nodeProgramClasses[m], c.nodeHoverProgramClasses[m]);
                  for (var g in i.nodeProgramClasses) c.nodeProgramClasses[g] || this.unregisterNodeProgram(g);
                }
              }
              return this.mouseCaptor.setSettings(this.settings), this.touchCaptor.setSettings(this.settings), this;
            }
          },
          {
            key: "cleanCameraState",
            value: function(i) {
              var u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {}, c = u.tolerance, d = c === void 0 ? 0 : c, h = u.boundaries, m = ve({}, i), g = h || this.nodeExtent, p = ka(g.x, 2), _ = p[0], E = p[1], R = ka(g.y, 2), z = R[0], H = R[1], Y = [
                this.graphToViewport({
                  x: _,
                  y: z
                }, {
                  cameraState: i
                }),
                this.graphToViewport({
                  x: E,
                  y: z
                }, {
                  cameraState: i
                }),
                this.graphToViewport({
                  x: _,
                  y: H
                }, {
                  cameraState: i
                }),
                this.graphToViewport({
                  x: E,
                  y: H
                }, {
                  cameraState: i
                })
              ], te = 1 / 0, J = -1 / 0, I = 1 / 0, G = -1 / 0;
              Y.forEach(function(ue) {
                var oe = ue.x, D = ue.y;
                te = Math.min(te, oe), J = Math.max(J, oe), I = Math.min(I, D), G = Math.max(G, D);
              });
              var B = J - te, Z = G - I, T = this.getDimensions(), y = T.width, w = T.height, S = 0, N = 0;
              if (B >= y ? J < y - d ? S = J - (y - d) : te > d && (S = te - d) : J > y + d ? S = J - (y + d) : te < -d && (S = te + d), Z >= w ? G < w - d ? N = G - (w - d) : I > d && (N = I - d) : G > w + d ? N = G - (w + d) : I < -d && (N = I + d), S || N) {
                var L = this.viewportToFramedGraph({
                  x: 0,
                  y: 0
                }, {
                  cameraState: i
                }), re = this.viewportToFramedGraph({
                  x: S,
                  y: N
                }, {
                  cameraState: i
                });
                S = re.x - L.x, N = re.y - L.y, m.x += S, m.y += N;
              }
              return m;
            }
          },
          {
            key: "renderLabels",
            value: function() {
              if (!this.settings.renderLabels) return this;
              var i = this.camera.getState(), u = this.labelGrid.getLabelsToDisplay(i.ratio, this.settings.labelDensity);
              Am(u, this.nodesWithForcedLabels), this.displayedNodeLabels = /* @__PURE__ */ new Set();
              for (var c = this.canvasContexts.labels, d = 0, h = u.length; d < h; d++) {
                var m = u[d], g = this.nodeDataCache[m];
                if (!this.displayedNodeLabels.has(m) && !g.hidden) {
                  var p = this.framedGraphToViewport(g), _ = p.x, E = p.y, R = this.scaleSize(g.size);
                  if (!(!g.forceLabel && R < this.settings.labelRenderedSizeThreshold) && !(_ < -150 || _ > this.width + GS || E < -50 || E > this.height + US)) {
                    this.displayedNodeLabels.add(m);
                    var z = this.settings.defaultDrawNodeLabel, H = this.nodePrograms[g.type], Y = (H == null ? void 0 : H.drawLabel) || z;
                    Y(c, ve(ve({
                      key: m
                    }, g), {}, {
                      size: R,
                      x: _,
                      y: E
                    }), this.settings);
                  }
                }
              }
              return this;
            }
          },
          {
            key: "renderEdgeLabels",
            value: function() {
              if (!this.settings.renderEdgeLabels) return this;
              var i = this.canvasContexts.edgeLabels;
              i.clearRect(0, 0, this.width, this.height);
              var u = LS({
                graph: this.graph,
                hoveredNode: this.hoveredNode,
                displayedNodeLabels: this.displayedNodeLabels,
                highlightedNodes: this.highlightedNodes
              });
              Am(u, this.edgesWithForcedLabels);
              for (var c = /* @__PURE__ */ new Set(), d = 0, h = u.length; d < h; d++) {
                var m = u[d], g = this.graph.extremities(m), p = this.nodeDataCache[g[0]], _ = this.nodeDataCache[g[1]], E = this.edgeDataCache[m];
                if (!c.has(m) && !(E.hidden || p.hidden || _.hidden)) {
                  var R = this.settings.defaultDrawEdgeLabel, z = this.edgePrograms[E.type], H = (z == null ? void 0 : z.drawLabel) || R;
                  H(i, ve(ve({
                    key: m
                  }, E), {}, {
                    size: this.scaleSize(E.size)
                  }), ve(ve(ve({
                    key: g[0]
                  }, p), this.framedGraphToViewport(p)), {}, {
                    size: this.scaleSize(p.size)
                  }), ve(ve(ve({
                    key: g[1]
                  }, _), this.framedGraphToViewport(_)), {}, {
                    size: this.scaleSize(_.size)
                  }), this.settings), c.add(m);
                }
              }
              return this.displayedEdgeLabels = c, this;
            }
          },
          {
            key: "renderHighlightedNodes",
            value: function() {
              var i = this, u = this.canvasContexts.hovers;
              u.clearRect(0, 0, this.width, this.height);
              var c = function(R) {
                var z = i.nodeDataCache[R], H = i.framedGraphToViewport(z), Y = H.x, te = H.y, J = i.scaleSize(z.size), I = i.settings.defaultDrawNodeHover, G = i.nodePrograms[z.type], B = (G == null ? void 0 : G.drawHover) || I;
                B(u, ve(ve({
                  key: R
                }, z), {}, {
                  size: J,
                  x: Y,
                  y: te
                }), i.settings);
              }, d = [];
              this.hoveredNode && !this.nodeDataCache[this.hoveredNode].hidden && d.push(this.hoveredNode), this.highlightedNodes.forEach(function(E) {
                E !== i.hoveredNode && d.push(E);
              }), d.forEach(function(E) {
                return c(E);
              });
              var h = {};
              d.forEach(function(E) {
                var R = i.nodeDataCache[E].type;
                h[R] = (h[R] || 0) + 1;
              });
              for (var m in this.nodeHoverPrograms) this.nodeHoverPrograms[m].reallocate(h[m] || 0), h[m] = 0;
              d.forEach(function(E) {
                var R = i.nodeDataCache[E];
                i.nodeHoverPrograms[R.type].process(0, h[R.type]++, R);
              }), this.webGLContexts.hoverNodes.clear(this.webGLContexts.hoverNodes.COLOR_BUFFER_BIT);
              var g = this.getRenderParams();
              for (var p in this.nodeHoverPrograms) {
                var _ = this.nodeHoverPrograms[p];
                _.render(g);
              }
            }
          },
          {
            key: "scheduleHighlightedNodesRender",
            value: function() {
              var i = this;
              this.renderHighlightedNodesFrame || this.renderFrame || (this.renderHighlightedNodesFrame = requestAnimationFrame(function() {
                i.renderHighlightedNodesFrame = null, i.renderHighlightedNodes(), i.renderEdgeLabels();
              }));
            }
          },
          {
            key: "render",
            value: function() {
              var i = this;
              this.emit("beforeRender");
              var u = function() {
                return i.emit("afterRender"), i;
              };
              if (this.renderFrame && (cancelAnimationFrame(this.renderFrame), this.renderFrame = null), this.resize(), this.needToProcess && this.process(), this.needToProcess = false, this.clear(), this.pickingLayers.forEach(function(Y) {
                return i.resetWebGLTexture(Y);
              }), !this.graph.order) return u();
              var c = this.mouseCaptor, d = this.camera.isAnimated() || c.isMoving || c.draggedEvents || c.currentWheelDirection, h = this.camera.getState(), m = this.getDimensions(), g = this.getGraphDimensions(), p = this.getStagePadding();
              this.matrix = Fr(h, m, g, p), this.invMatrix = Fr(h, m, g, p, true), this.correctionRatio = vS(this.matrix, h, m), this.graphToViewportRatio = this.getGraphToViewportRatio();
              var _ = this.getRenderParams();
              for (var E in this.nodePrograms) {
                var R = this.nodePrograms[E];
                R.render(_);
              }
              if (!this.settings.hideEdgesOnMove || !d) for (var z in this.edgePrograms) {
                var H = this.edgePrograms[z];
                H.render(_);
              }
              return this.settings.hideLabelsOnMove && d || (this.renderLabels(), this.renderEdgeLabels(), this.renderHighlightedNodes()), u();
            }
          },
          {
            key: "addNode",
            value: function(i) {
              var u = Object.assign({}, this.graph.getNodeAttributes(i));
              this.settings.nodeReducer && (u = this.settings.nodeReducer(i, u));
              var c = jS(this.settings, i, u);
              this.nodeDataCache[i] = c, this.nodesWithForcedLabels.delete(i), c.forceLabel && !c.hidden && this.nodesWithForcedLabels.add(i), this.highlightedNodes.delete(i), c.highlighted && !c.hidden && this.highlightedNodes.add(i), this.settings.zIndex && (c.zIndex < this.nodeZExtent[0] && (this.nodeZExtent[0] = c.zIndex), c.zIndex > this.nodeZExtent[1] && (this.nodeZExtent[1] = c.zIndex));
            }
          },
          {
            key: "updateNode",
            value: function(i) {
              this.addNode(i);
              var u = this.nodeDataCache[i];
              this.normalizationFunction.applyTo(u);
            }
          },
          {
            key: "removeNode",
            value: function(i) {
              delete this.nodeDataCache[i], delete this.nodeProgramIndex[i], this.highlightedNodes.delete(i), this.hoveredNode === i && (this.hoveredNode = null), this.nodesWithForcedLabels.delete(i);
            }
          },
          {
            key: "addEdge",
            value: function(i) {
              var u = Object.assign({}, this.graph.getEdgeAttributes(i));
              this.settings.edgeReducer && (u = this.settings.edgeReducer(i, u));
              var c = BS(this.settings, i, u);
              this.edgeDataCache[i] = c, this.edgesWithForcedLabels.delete(i), c.forceLabel && !c.hidden && this.edgesWithForcedLabels.add(i), this.settings.zIndex && (c.zIndex < this.edgeZExtent[0] && (this.edgeZExtent[0] = c.zIndex), c.zIndex > this.edgeZExtent[1] && (this.edgeZExtent[1] = c.zIndex));
            }
          },
          {
            key: "updateEdge",
            value: function(i) {
              this.addEdge(i);
            }
          },
          {
            key: "removeEdge",
            value: function(i) {
              delete this.edgeDataCache[i], delete this.edgeProgramIndex[i], this.hoveredEdge === i && (this.hoveredEdge = null), this.edgesWithForcedLabels.delete(i);
            }
          },
          {
            key: "clearNodeIndices",
            value: function() {
              this.labelGrid = new Nm(), this.nodeExtent = {
                x: [
                  0,
                  1
                ],
                y: [
                  0,
                  1
                ]
              }, this.nodeDataCache = {}, this.edgeProgramIndex = {}, this.nodesWithForcedLabels = /* @__PURE__ */ new Set(), this.nodeZExtent = [
                1 / 0,
                -1 / 0
              ], this.highlightedNodes = /* @__PURE__ */ new Set();
            }
          },
          {
            key: "clearEdgeIndices",
            value: function() {
              this.edgeDataCache = {}, this.edgeProgramIndex = {}, this.edgesWithForcedLabels = /* @__PURE__ */ new Set(), this.edgeZExtent = [
                1 / 0,
                -1 / 0
              ];
            }
          },
          {
            key: "clearIndices",
            value: function() {
              this.clearEdgeIndices(), this.clearNodeIndices();
            }
          },
          {
            key: "clearNodeState",
            value: function() {
              this.displayedNodeLabels = /* @__PURE__ */ new Set(), this.highlightedNodes = /* @__PURE__ */ new Set(), this.hoveredNode = null;
            }
          },
          {
            key: "clearEdgeState",
            value: function() {
              this.displayedEdgeLabels = /* @__PURE__ */ new Set(), this.highlightedNodes = /* @__PURE__ */ new Set(), this.hoveredEdge = null;
            }
          },
          {
            key: "clearState",
            value: function() {
              this.clearEdgeState(), this.clearNodeState();
            }
          },
          {
            key: "addNodeToProgram",
            value: function(i, u, c) {
              var d = this.nodeDataCache[i], h = this.nodePrograms[d.type];
              if (!h) throw new Error('Sigma: could not find a suitable program for node type "'.concat(d.type, '"!'));
              h.process(u, c, d), this.nodeProgramIndex[i] = c;
            }
          },
          {
            key: "addEdgeToProgram",
            value: function(i, u, c) {
              var d = this.edgeDataCache[i], h = this.edgePrograms[d.type];
              if (!h) throw new Error('Sigma: could not find a suitable program for edge type "'.concat(d.type, '"!'));
              var m = this.graph.extremities(i), g = this.nodeDataCache[m[0]], p = this.nodeDataCache[m[1]];
              h.process(u, c, g, p, d), this.edgeProgramIndex[i] = c;
            }
          },
          {
            key: "getRenderParams",
            value: function() {
              return {
                matrix: this.matrix,
                invMatrix: this.invMatrix,
                width: this.width,
                height: this.height,
                pixelRatio: this.pixelRatio,
                zoomRatio: this.camera.ratio,
                cameraAngle: this.camera.angle,
                sizeRatio: 1 / this.scaleSize(),
                correctionRatio: this.correctionRatio,
                downSizingRatio: this.pickingDownSizingRatio,
                minEdgeThickness: this.settings.minEdgeThickness,
                antiAliasingFeather: this.settings.antiAliasingFeather
              };
            }
          },
          {
            key: "getStagePadding",
            value: function() {
              var i = this.settings, u = i.stagePadding, c = i.autoRescale;
              return c && u || 0;
            }
          },
          {
            key: "createLayer",
            value: function(i, u) {
              var c = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : {};
              if (this.elements[i]) throw new Error('Sigma: a layer named "'.concat(i, '" already exists'));
              var d = bS(u, {
                position: "absolute"
              }, {
                class: "sigma-".concat(i)
              });
              return c.style && Object.assign(d.style, c.style), this.elements[i] = d, "beforeLayer" in c && c.beforeLayer ? this.elements[c.beforeLayer].before(d) : "afterLayer" in c && c.afterLayer ? this.elements[c.afterLayer].after(d) : this.container.appendChild(d), d;
            }
          },
          {
            key: "createCanvas",
            value: function(i) {
              var u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {};
              return this.createLayer(i, "canvas", u);
            }
          },
          {
            key: "createCanvasContext",
            value: function(i) {
              var u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {}, c = this.createCanvas(i, u), d = {
                preserveDrawingBuffer: false,
                antialias: false
              };
              return this.canvasContexts[i] = c.getContext("2d", d), this;
            }
          },
          {
            key: "createWebGLContext",
            value: function(i) {
              var u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {}, c = (u == null ? void 0 : u.canvas) || this.createCanvas(i, u);
              u.hidden && c.remove();
              var d = ve({
                preserveDrawingBuffer: false,
                antialias: false
              }, u), h;
              h = c.getContext("webgl2", d), h || (h = c.getContext("webgl", d)), h || (h = c.getContext("experimental-webgl", d));
              var m = h;
              if (this.webGLContexts[i] = m, m.blendFunc(m.ONE, m.ONE_MINUS_SRC_ALPHA), u.picking) {
                this.pickingLayers.add(i);
                var g = m.createFramebuffer();
                if (!g) throw new Error("Sigma: cannot create a new frame buffer for layer ".concat(i));
                this.frameBuffers[i] = g;
              }
              return m;
            }
          },
          {
            key: "killLayer",
            value: function(i) {
              var u = this.elements[i];
              if (!u) throw new Error("Sigma: cannot kill layer ".concat(i, ", which does not exist"));
              if (this.webGLContexts[i]) {
                var c, d = this.webGLContexts[i];
                (c = d.getExtension("WEBGL_lose_context")) === null || c === void 0 || c.loseContext(), delete this.webGLContexts[i];
              } else this.canvasContexts[i] && delete this.canvasContexts[i];
              return u.remove(), delete this.elements[i], this;
            }
          },
          {
            key: "getCamera",
            value: function() {
              return this.camera;
            }
          },
          {
            key: "setCamera",
            value: function(i) {
              this.unbindCameraHandlers(), this.camera = i, this.bindCameraHandlers();
            }
          },
          {
            key: "getContainer",
            value: function() {
              return this.container;
            }
          },
          {
            key: "getGraph",
            value: function() {
              return this.graph;
            }
          },
          {
            key: "setGraph",
            value: function(i) {
              i !== this.graph && (this.hoveredNode && !i.hasNode(this.hoveredNode) && (this.hoveredNode = null), this.hoveredEdge && !i.hasEdge(this.hoveredEdge) && (this.hoveredEdge = null), this.unbindGraphHandlers(), this.checkEdgesEventsFrame !== null && (cancelAnimationFrame(this.checkEdgesEventsFrame), this.checkEdgesEventsFrame = null), this.graph = i, this.bindGraphHandlers(), this.refresh());
            }
          },
          {
            key: "getMouseCaptor",
            value: function() {
              return this.mouseCaptor;
            }
          },
          {
            key: "getTouchCaptor",
            value: function() {
              return this.touchCaptor;
            }
          },
          {
            key: "getDimensions",
            value: function() {
              return {
                width: this.width,
                height: this.height
              };
            }
          },
          {
            key: "getGraphDimensions",
            value: function() {
              var i = this.customBBox || this.nodeExtent;
              return {
                width: i.x[1] - i.x[0] || 1,
                height: i.y[1] - i.y[0] || 1
              };
            }
          },
          {
            key: "getNodeDisplayData",
            value: function(i) {
              var u = this.nodeDataCache[i];
              return u ? Object.assign({}, u) : void 0;
            }
          },
          {
            key: "getEdgeDisplayData",
            value: function(i) {
              var u = this.edgeDataCache[i];
              return u ? Object.assign({}, u) : void 0;
            }
          },
          {
            key: "getNodeDisplayedLabels",
            value: function() {
              return new Set(this.displayedNodeLabels);
            }
          },
          {
            key: "getEdgeDisplayedLabels",
            value: function() {
              return new Set(this.displayedEdgeLabels);
            }
          },
          {
            key: "getSettings",
            value: function() {
              return ve({}, this.settings);
            }
          },
          {
            key: "getSetting",
            value: function(i) {
              return this.settings[i];
            }
          },
          {
            key: "setSetting",
            value: function(i, u) {
              var c = ve({}, this.settings);
              return this.settings[i] = u, fc(this.settings), this.handleSettingsUpdate(c), this.scheduleRefresh(), this;
            }
          },
          {
            key: "updateSetting",
            value: function(i, u) {
              return this.setSetting(i, u(this.settings[i])), this;
            }
          },
          {
            key: "setSettings",
            value: function(i) {
              var u = ve({}, this.settings);
              return this.settings = ve(ve({}, this.settings), i), fc(this.settings), this.handleSettingsUpdate(u), this.scheduleRefresh(), this;
            }
          },
          {
            key: "resize",
            value: function(i) {
              var u = this.width, c = this.height;
              if (this.width = this.container.offsetWidth, this.height = this.container.offsetHeight, this.pixelRatio = Sm(), this.width === 0) if (this.settings.allowInvalidContainer) this.width = 1;
              else throw new Error("Sigma: Container has no width. You can set the allowInvalidContainer setting to true to stop seeing this error.");
              if (this.height === 0) if (this.settings.allowInvalidContainer) this.height = 1;
              else throw new Error("Sigma: Container has no height. You can set the allowInvalidContainer setting to true to stop seeing this error.");
              if (!i && u === this.width && c === this.height) return this;
              for (var d in this.elements) {
                var h = this.elements[d];
                h.style.width = this.width + "px", h.style.height = this.height + "px";
              }
              for (var m in this.canvasContexts) this.elements[m].setAttribute("width", this.width * this.pixelRatio + "px"), this.elements[m].setAttribute("height", this.height * this.pixelRatio + "px"), this.pixelRatio !== 1 && this.canvasContexts[m].scale(this.pixelRatio, this.pixelRatio);
              for (var g in this.webGLContexts) {
                this.elements[g].setAttribute("width", this.width * this.pixelRatio + "px"), this.elements[g].setAttribute("height", this.height * this.pixelRatio + "px");
                var p = this.webGLContexts[g];
                if (p.viewport(0, 0, this.width * this.pixelRatio, this.height * this.pixelRatio), this.pickingLayers.has(g)) {
                  var _ = this.textures[g];
                  _ && p.deleteTexture(_);
                }
              }
              return this.emit("resize"), this;
            }
          },
          {
            key: "clear",
            value: function() {
              return this.emit("beforeClear"), this.webGLContexts.nodes.bindFramebuffer(WebGLRenderingContext.FRAMEBUFFER, null), this.webGLContexts.nodes.clear(WebGLRenderingContext.COLOR_BUFFER_BIT), this.webGLContexts.edges.bindFramebuffer(WebGLRenderingContext.FRAMEBUFFER, null), this.webGLContexts.edges.clear(WebGLRenderingContext.COLOR_BUFFER_BIT), this.webGLContexts.hoverNodes.clear(WebGLRenderingContext.COLOR_BUFFER_BIT), this.canvasContexts.labels.clearRect(0, 0, this.width, this.height), this.canvasContexts.hovers.clearRect(0, 0, this.width, this.height), this.canvasContexts.edgeLabels.clearRect(0, 0, this.width, this.height), this.emit("afterClear"), this;
            }
          },
          {
            key: "refresh",
            value: function(i) {
              var u = this, c = (i == null ? void 0 : i.skipIndexation) !== void 0 ? i == null ? void 0 : i.skipIndexation : false, d = (i == null ? void 0 : i.schedule) !== void 0 ? i.schedule : false, h = !i || !i.partialGraph;
              if (h) this.clearEdgeIndices(), this.clearNodeIndices(), this.graph.forEachNode(function(G) {
                return u.addNode(G);
              }), this.graph.forEachEdge(function(G) {
                return u.addEdge(G);
              });
              else {
                for (var m, g, p = ((m = i.partialGraph) === null || m === void 0 ? void 0 : m.nodes) || [], _ = 0, E = (p == null ? void 0 : p.length) || 0; _ < E; _++) {
                  var R = p[_];
                  if (this.updateNode(R), c) {
                    var z = this.nodeProgramIndex[R];
                    if (z === void 0) throw new Error('Sigma: node "'.concat(R, `" can't be repaint`));
                    this.addNodeToProgram(R, this.nodeIndices[R], z);
                  }
                }
                for (var H = (i == null || (g = i.partialGraph) === null || g === void 0 ? void 0 : g.edges) || [], Y = 0, te = H.length; Y < te; Y++) {
                  var J = H[Y];
                  if (this.updateEdge(J), c) {
                    var I = this.edgeProgramIndex[J];
                    if (I === void 0) throw new Error('Sigma: edge "'.concat(J, `" can't be repaint`));
                    this.addEdgeToProgram(J, this.edgeIndices[J], I);
                  }
                }
              }
              return (h || !c) && (this.needToProcess = true), d ? this.scheduleRender() : this.render(), this;
            }
          },
          {
            key: "scheduleRender",
            value: function() {
              var i = this;
              return this.renderFrame || (this.renderFrame = requestAnimationFrame(function() {
                i.render();
              })), this;
            }
          },
          {
            key: "scheduleRefresh",
            value: function(i) {
              return this.refresh(ve(ve({}, i), {}, {
                schedule: true
              }));
            }
          },
          {
            key: "getViewportZoomedState",
            value: function(i, u) {
              var c = this.camera.getState(), d = c.ratio, h = c.angle, m = c.x, g = c.y, p = this.settings, _ = p.minCameraRatio, E = p.maxCameraRatio;
              typeof E == "number" && (u = Math.min(u, E)), typeof _ == "number" && (u = Math.max(u, _));
              var R = u / d, z = {
                x: this.width / 2,
                y: this.height / 2
              }, H = this.viewportToFramedGraph(i), Y = this.viewportToFramedGraph(z);
              return {
                angle: h,
                x: (H.x - Y.x) * (1 - R) + m,
                y: (H.y - Y.y) * (1 - R) + g,
                ratio: u
              };
            }
          },
          {
            key: "viewRectangle",
            value: function() {
              var i = this.viewportToFramedGraph({
                x: 0,
                y: 0
              }), u = this.viewportToFramedGraph({
                x: this.width,
                y: 0
              }), c = this.viewportToFramedGraph({
                x: 0,
                y: this.height
              });
              return {
                x1: i.x,
                y1: i.y,
                x2: u.x,
                y2: u.y,
                height: u.y - c.y
              };
            }
          },
          {
            key: "framedGraphToViewport",
            value: function(i) {
              var u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {}, c = !!u.cameraState || !!u.viewportDimensions || !!u.graphDimensions, d = u.matrix ? u.matrix : c ? Fr(u.cameraState || this.camera.getState(), u.viewportDimensions || this.getDimensions(), u.graphDimensions || this.getGraphDimensions(), u.padding || this.getStagePadding()) : this.matrix, h = Nc(d, i);
              return {
                x: (1 + h.x) * this.width / 2,
                y: (1 - h.y) * this.height / 2
              };
            }
          },
          {
            key: "viewportToFramedGraph",
            value: function(i) {
              var u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {}, c = !!u.cameraState || !!u.viewportDimensions || !u.graphDimensions, d = u.matrix ? u.matrix : c ? Fr(u.cameraState || this.camera.getState(), u.viewportDimensions || this.getDimensions(), u.graphDimensions || this.getGraphDimensions(), u.padding || this.getStagePadding(), true) : this.invMatrix, h = Nc(d, {
                x: i.x / this.width * 2 - 1,
                y: 1 - i.y / this.height * 2
              });
              return isNaN(h.x) && (h.x = 0), isNaN(h.y) && (h.y = 0), h;
            }
          },
          {
            key: "viewportToGraph",
            value: function(i) {
              var u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {};
              return this.normalizationFunction.inverse(this.viewportToFramedGraph(i, u));
            }
          },
          {
            key: "graphToViewport",
            value: function(i) {
              var u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : {};
              return this.framedGraphToViewport(this.normalizationFunction(i), u);
            }
          },
          {
            key: "getGraphToViewportRatio",
            value: function() {
              var i = {
                x: 0,
                y: 0
              }, u = {
                x: 1,
                y: 1
              }, c = Math.sqrt(Math.pow(i.x - u.x, 2) + Math.pow(i.y - u.y, 2)), d = this.graphToViewport(i), h = this.graphToViewport(u), m = Math.sqrt(Math.pow(d.x - h.x, 2) + Math.pow(d.y - h.y, 2));
              return m / c;
            }
          },
          {
            key: "getBBox",
            value: function() {
              return this.nodeExtent;
            }
          },
          {
            key: "getCustomBBox",
            value: function() {
              return this.customBBox;
            }
          },
          {
            key: "setCustomBBox",
            value: function(i) {
              return this.customBBox = i, this.scheduleRender(), this;
            }
          },
          {
            key: "kill",
            value: function() {
              this.emit("kill"), this.removeAllListeners(), this.unbindCameraHandlers(), window.removeEventListener("resize", this.activeListeners.handleResize), this.mouseCaptor.kill(), this.touchCaptor.kill(), this.unbindGraphHandlers(), this.clearIndices(), this.clearState(), this.nodeDataCache = {}, this.edgeDataCache = {}, this.highlightedNodes.clear(), this.renderFrame && (cancelAnimationFrame(this.renderFrame), this.renderFrame = null), this.renderHighlightedNodesFrame && (cancelAnimationFrame(this.renderHighlightedNodesFrame), this.renderHighlightedNodesFrame = null);
              for (var i = this.container; i.firstChild; ) i.removeChild(i.firstChild);
              for (var u in this.nodePrograms) this.nodePrograms[u].kill();
              for (var c in this.nodeHoverPrograms) this.nodeHoverPrograms[c].kill();
              for (var d in this.edgePrograms) this.edgePrograms[d].kill();
              this.nodePrograms = {}, this.nodeHoverPrograms = {}, this.edgePrograms = {};
              for (var h in this.elements) this.killLayer(h);
              this.canvasContexts = {}, this.webGLContexts = {}, this.elements = {};
            }
          },
          {
            key: "scaleSize",
            value: function() {
              var i = arguments.length > 0 && arguments[0] !== void 0 ? arguments[0] : 1, u = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : this.camera.ratio;
              return i / this.settings.zoomToSizeRatioFunction(u) * (this.getSetting("itemSizesReference") === "positions" ? u * this.graphToViewportRatio : 1);
            }
          },
          {
            key: "getCanvases",
            value: function() {
              var i = {};
              for (var u in this.elements) this.elements[u] instanceof HTMLCanvasElement && (i[u] = this.elements[u]);
              return i;
            }
          }
        ]);
      }(Pc);
      const ep = ae.createContext(null), FS = ep.Provider;
      function tp() {
        const l = ae.useContext(ep);
        if (l == null) throw new Error("No context provided: useSigmaContext() can only be used in a descendant of <SigmaContainer>");
        return l;
      }
      function np() {
        return tp().sigma;
      }
      function qS() {
        const { sigma: l } = tp();
        return ae.useCallback((a) => {
          l && Object.keys(a).forEach((o) => {
            l.setSetting(o, a[o]);
          });
        }, [
          l
        ]);
      }
      function zo(l) {
        return new Set(Object.keys(l));
      }
      const Om = zo({
        clickNode: true,
        rightClickNode: true,
        downNode: true,
        enterNode: true,
        leaveNode: true,
        doubleClickNode: true,
        wheelNode: true,
        clickEdge: true,
        rightClickEdge: true,
        downEdge: true,
        enterEdge: true,
        leaveEdge: true,
        doubleClickEdge: true,
        wheelEdge: true,
        clickStage: true,
        rightClickStage: true,
        downStage: true,
        doubleClickStage: true,
        wheelStage: true,
        beforeRender: true,
        afterRender: true,
        kill: true,
        upStage: true,
        upEdge: true,
        upNode: true,
        enterStage: true,
        leaveStage: true,
        resize: true,
        afterClear: true,
        afterProcess: true,
        beforeClear: true,
        beforeProcess: true,
        moveBody: true
      }), zm = zo({
        click: true,
        rightClick: true,
        doubleClick: true,
        mouseup: true,
        mousedown: true,
        mousemove: true,
        mousemovebody: true,
        mouseleave: true,
        mouseenter: true,
        wheel: true
      }), km = zo({
        touchup: true,
        touchdown: true,
        touchmove: true,
        touchmovebody: true,
        tap: true,
        doubletap: true
      }), Mm = zo({
        updated: true
      });
      function VS() {
        const l = np(), a = qS(), [o, i] = ae.useState({});
        return ae.useEffect(() => {
          if (!l || !o) return;
          const u = o, c = Object.keys(u);
          return c.forEach((d) => {
            const h = u[d];
            Om.has(d) && l.on(d, h), zm.has(d) && l.getMouseCaptor().on(d, h), km.has(d) && l.getTouchCaptor().on(d, h), Mm.has(d) && l.getCamera().on(d, h);
          }), () => {
            l && c.forEach((d) => {
              const h = u[d];
              Om.has(d) && l.off(d, h), zm.has(d) && l.getMouseCaptor().off(d, h), km.has(d) && l.getTouchCaptor().off(d, h), Mm.has(d) && l.getCamera().off(d, h);
            });
          };
        }, [
          l,
          o,
          a
        ]), i;
      }
      function YS() {
        const l = np();
        return ae.useCallback((a, o = true) => {
          l && a && (o && l.getGraph().order > 0 && l.getGraph().clear(), l.getGraph().import(a), l.refresh());
        }, [
          l
        ]);
      }
      function ip(l, a) {
        if (l === a) return true;
        if (typeof l == "object" && l != null && typeof a == "object" && a != null) {
          if (Object.keys(l).length != Object.keys(a).length) return false;
          for (const o in l) if (!Object.hasOwn(a, o) || !ip(l[o], a[o])) return false;
          return true;
        }
        return false;
      }
      const $S = ae.forwardRef(({ graph: l, id: a, className: o, style: i, settings: u = {}, children: c }, d) => {
        const h = ae.useRef(null), m = ae.useRef(null), g = {
          className: `react-sigma ${o || ""}`,
          id: a,
          style: i
        }, [p, _] = ae.useState(null), [E, R] = ae.useState(u);
        ae.useEffect(() => {
          R((Y) => ip(Y, u) ? Y : u);
        }, [
          u
        ]), ae.useEffect(() => {
          _((Y) => {
            let te = null;
            if (m.current !== null) {
              let J = new We();
              l && (J = typeof l == "function" ? new l() : l);
              let I = null;
              Y && (I = Y.getCamera().getState(), Y.kill()), te = new HS(J, m.current, E), I && te.getCamera().setState(I);
            }
            return te;
          });
        }, [
          m,
          l,
          E
        ]), ae.useImperativeHandle(d, () => p, [
          p
        ]);
        const z = ae.useMemo(() => p && h.current ? {
          sigma: p,
          container: h.current
        } : null, [
          p,
          h
        ]), H = z !== null ? xn.createElement(FS, {
          value: z
        }, c) : null;
        return xn.createElement("div", Object.assign({}, g, {
          ref: h
        }), xn.createElement("div", {
          className: "sigma-container",
          ref: m
        }), H);
      });
      var Da = {}, hc, Lm;
      function ko() {
        if (Lm) return hc;
        Lm = 1;
        function l(o) {
          return !o || typeof o != "object" || typeof o == "function" || Array.isArray(o) || o instanceof Set || o instanceof Map || o instanceof RegExp || o instanceof Date;
        }
        function a(o, i) {
          o = o || {};
          var u = {};
          for (var c in i) {
            var d = o[c], h = i[c];
            if (!l(h)) {
              u[c] = a(d, h);
              continue;
            }
            d === void 0 ? u[c] = h : u[c] = d;
          }
          return u;
        }
        return hc = a, hc;
      }
      var gc, Gm;
      function XS() {
        if (Gm) return gc;
        Gm = 1;
        function l(o) {
          return function(i, u) {
            return i + Math.floor(o() * (u - i + 1));
          };
        }
        var a = l(Math.random);
        return a.createRandom = l, gc = a, gc;
      }
      var mc, Um;
      function ZS() {
        if (Um) return mc;
        Um = 1;
        var l = XS().createRandom;
        function a(i) {
          var u = l(i);
          return function(c) {
            for (var d = c.length, h = d - 1, m = -1; ++m < d; ) {
              var g = u(m, h), p = c[g];
              c[g] = c[m], c[m] = p;
            }
          };
        }
        var o = a(Math.random);
        return o.createShuffleInPlace = a, mc = o, mc;
      }
      var vc, jm;
      function QS() {
        if (jm) return vc;
        jm = 1;
        var l = ko(), a = Pr(), o = ZS(), i = {
          attributes: {
            x: "x",
            y: "y"
          },
          center: 0,
          hierarchyAttributes: [],
          rng: Math.random,
          scale: 1
        };
        function u(T, y, w, S, N) {
          this.wrappedCircle = N || null, this.children = {}, this.countChildren = 0, this.id = T || null, this.next = null, this.previous = null, this.x = y || null, this.y = w || null, N ? this.r = 1010101 : this.r = S || 999;
        }
        u.prototype.hasChildren = function() {
          return this.countChildren > 0;
        }, u.prototype.addChild = function(T, y) {
          this.children[T] = y, ++this.countChildren;
        }, u.prototype.getChild = function(T) {
          if (!this.children.hasOwnProperty(T)) {
            var y = new u();
            this.children[T] = y, ++this.countChildren;
          }
          return this.children[T];
        }, u.prototype.applyPositionToChildren = function() {
          if (this.hasChildren()) {
            var T = this;
            for (var y in T.children) {
              var w = T.children[y];
              w.x += T.x, w.y += T.y, w.applyPositionToChildren();
            }
          }
        };
        function c(T, y, w) {
          for (var S in y.children) {
            var N = y.children[S];
            N.hasChildren() ? c(T, N, w) : w[N.id] = {
              x: N.x,
              y: N.y
            };
          }
        }
        function d(T, y) {
          var w = T.r - y.r, S = y.x - T.x, N = y.y - T.y;
          return w < 0 || w * w < S * S + N * N;
        }
        function h(T, y) {
          var w = T.r - y.r + 1e-6, S = y.x - T.x, N = y.y - T.y;
          return w > 0 && w * w > S * S + N * N;
        }
        function m(T, y) {
          for (var w = 0; w < y.length; ++w) if (!h(T, y[w])) return false;
          return true;
        }
        function g(T) {
          return new u(null, T.x, T.y, T.r);
        }
        function p(T, y) {
          var w = T.x, S = T.y, N = T.r, L = y.x, re = y.y, ue = y.r, oe = L - w, D = re - S, X = ue - N, $ = Math.sqrt(oe * oe + D * D);
          return new u(null, (w + L + oe / $ * X) / 2, (S + re + D / $ * X) / 2, ($ + N + ue) / 2);
        }
        function _(T, y, w) {
          var S = T.x, N = T.y, L = T.r, re = y.x, ue = y.y, oe = y.r, D = w.x, X = w.y, $ = w.r, ne = S - re, x = S - D, k = N - ue, K = N - X, U = oe - L, ie = $ - L, de = S * S + N * N - L * L, ce = de - re * re - ue * ue + oe * oe, be = de - D * D - X * X + $ * $, fe = x * k - ne * K, _e = (k * be - K * ce) / (fe * 2) - S, ke = (K * U - k * ie) / fe, pe = (x * ce - ne * be) / (fe * 2) - N, Me = (ne * ie - x * U) / fe, Ze = ke * ke + Me * Me - 1, Le = 2 * (L + _e * ke + pe * Me), je = _e * _e + pe * pe - L * L, Ae = -(Ze ? (Le + Math.sqrt(Le * Le - 4 * Ze * je)) / (2 * Ze) : je / Le);
          return new u(null, S + _e + ke * Ae, N + pe + Me * Ae, Ae);
        }
        function E(T) {
          switch (T.length) {
            case 1:
              return g(T[0]);
            case 2:
              return p(T[0], T[1]);
            case 3:
              return _(T[0], T[1], T[2]);
            default:
              throw new Error("graphology-layout/circlepack: Invalid basis length " + T.length);
          }
        }
        function R(T, y) {
          var w, S;
          if (m(y, T)) return [
            y
          ];
          for (w = 0; w < T.length; ++w) if (d(y, T[w]) && m(p(T[w], y), T)) return [
            T[w],
            y
          ];
          for (w = 0; w < T.length - 1; ++w) for (S = w + 1; S < T.length; ++S) if (d(p(T[w], T[S]), y) && d(p(T[w], y), T[S]) && d(p(T[S], y), T[w]) && m(_(T[w], T[S], y), T)) return [
            T[w],
            T[S],
            y
          ];
          throw new Error("graphology-layout/circlepack: extendBasis failure !");
        }
        function z(T) {
          var y = T.wrappedCircle, w = T.next.wrappedCircle, S = y.r + w.r, N = (y.x * w.r + w.x * y.r) / S, L = (y.y * w.r + w.y * y.r) / S;
          return N * N + L * L;
        }
        function H(T, y) {
          var w = 0, S = T.slice(), N = T.length, L = [], re, ue;
          for (y(S); w < N; ) re = S[w], ue && h(ue, re) ? ++w : (L = R(L, re), ue = E(L), w = 0);
          return ue;
        }
        function Y(T, y, w) {
          var S = T.x - y.x, N, L, re = T.y - y.y, ue, oe, D = S * S + re * re;
          D ? (L = y.r + w.r, L *= L, oe = T.r + w.r, oe *= oe, L > oe ? (N = (D + oe - L) / (2 * D), ue = Math.sqrt(Math.max(0, oe / D - N * N)), w.x = T.x - N * S - ue * re, w.y = T.y - N * re + ue * S) : (N = (D + L - oe) / (2 * D), ue = Math.sqrt(Math.max(0, L / D - N * N)), w.x = y.x + N * S - ue * re, w.y = y.y + N * re + ue * S)) : (w.x = y.x + w.r, w.y = y.y);
        }
        function te(T, y) {
          var w = T.r + y.r - 1e-6, S = y.x - T.x, N = y.y - T.y;
          return w > 0 && w * w > S * S + N * N;
        }
        function J(T, y) {
          var w = T.length;
          if (w === 0) return 0;
          var S, N, L, re, ue, oe, D, X, $, ne;
          if (S = T[0], S.x = 0, S.y = 0, w <= 1) return S.r;
          if (N = T[1], S.x = -N.r, N.x = S.r, N.y = 0, w <= 2) return S.r + N.r;
          L = T[2], Y(N, S, L), S = new u(null, null, null, null, S), N = new u(null, null, null, null, N), L = new u(null, null, null, null, L), S.next = L.previous = N, N.next = S.previous = L, L.next = N.previous = S;
          e: for (oe = 3; oe < w; ++oe) {
            L = T[oe], Y(S.wrappedCircle, N.wrappedCircle, L), L = new u(null, null, null, null, L), D = N.next, X = S.previous, $ = N.wrappedCircle.r, ne = S.wrappedCircle.r;
            do
              if ($ <= ne) {
                if (te(D.wrappedCircle, L.wrappedCircle)) {
                  N = D, S.next = N, N.previous = S, --oe;
                  continue e;
                }
                $ += D.wrappedCircle.r, D = D.next;
              } else {
                if (te(X.wrappedCircle, L.wrappedCircle)) {
                  S = X, S.next = N, N.previous = S, --oe;
                  continue e;
                }
                ne += X.wrappedCircle.r, X = X.previous;
              }
            while (D !== X.next);
            for (L.previous = S, L.next = N, S.next = N.previous = N = L, re = z(S); (L = L.next) !== N; ) (ue = z(L)) < re && (S = L, re = ue);
            N = S.next;
          }
          S = [
            N.wrappedCircle
          ], L = N;
          for (var x = 1e4; (L = L.next) !== N && --x !== 0; ) S.push(L.wrappedCircle);
          for (L = H(S, y), oe = 0; oe < w; ++oe) S = T[oe], S.x -= L.x, S.y -= L.y;
          return L.r;
        }
        function I(T, y) {
          var w = 0;
          if (T.hasChildren()) {
            for (var S in T.children) {
              var N = T.children[S];
              N.hasChildren() && (N.r = I(N, y));
            }
            w = J(Object.values(T.children), y);
          }
          return w;
        }
        function G(T, y) {
          I(T, y);
          for (var w in T.children) {
            var S = T.children[w];
            S.applyPositionToChildren();
          }
        }
        function B(T, y, w) {
          if (!a(y)) throw new Error("graphology-layout/circlepack: the given graph is not a valid graphology instance.");
          w = l(w, i);
          var S = {}, N = {}, L = y.nodes(), re = w.center, ue = w.hierarchyAttributes, oe = o.createShuffleInPlace(w.rng), D = w.scale, X = new u();
          y.forEachNode(function(U, ie) {
            var de = ie.size ? ie.size : 1, ce = new u(U, null, null, de), be = X;
            ue.forEach(function(fe) {
              var _e = ie[fe];
              be = be.getChild(_e);
            }), be.addChild(U, ce);
          }), G(X, oe), c(y, X, S);
          var $ = L.length, ne, x, k;
          for (k = 0; k < $; k++) {
            var K = L[k];
            ne = re + D * S[K].x, x = re + D * S[K].y, N[K] = {
              x: ne,
              y: x
            }, T && (y.setNodeAttribute(K, w.attributes.x, ne), y.setNodeAttribute(K, w.attributes.y, x));
          }
          return N;
        }
        var Z = B.bind(null, false);
        return Z.assign = B.bind(null, true), vc = Z, vc;
      }
      var pc, Bm;
      function KS() {
        if (Bm) return pc;
        Bm = 1;
        var l = ko(), a = Pr(), o = {
          dimensions: [
            "x",
            "y"
          ],
          center: 0.5,
          scale: 1
        };
        function i(c, d, h) {
          if (!a(d)) throw new Error("graphology-layout/random: the given graph is not a valid graphology instance.");
          h = l(h, o);
          var m = h.dimensions;
          if (!Array.isArray(m) || m.length !== 2) throw new Error("graphology-layout/random: given dimensions are invalid.");
          var g = h.center, p = h.scale, _ = Math.PI * 2, E = (g - 0.5) * p, R = d.order, z = m[0], H = m[1];
          function Y(I, G) {
            return G[z] = p * Math.cos(I * _ / R) + E, G[H] = p * Math.sin(I * _ / R) + E, G;
          }
          var te = 0;
          if (!c) {
            var J = {};
            return d.forEachNode(function(I) {
              J[I] = Y(te++, {});
            }), J;
          }
          d.updateEachNodeAttributes(function(I, G) {
            return Y(te++, G), G;
          }, {
            attributes: m
          });
        }
        var u = i.bind(null, false);
        return u.assign = i.bind(null, true), pc = u, pc;
      }
      var yc, Hm;
      function PS() {
        if (Hm) return yc;
        Hm = 1;
        var l = ko(), a = Pr(), o = {
          dimensions: [
            "x",
            "y"
          ],
          center: 0.5,
          rng: Math.random,
          scale: 1
        };
        function i(c, d, h) {
          if (!a(d)) throw new Error("graphology-layout/random: the given graph is not a valid graphology instance.");
          h = l(h, o);
          var m = h.dimensions;
          if (!Array.isArray(m) || m.length < 1) throw new Error("graphology-layout/random: given dimensions are invalid.");
          var g = m.length, p = h.center, _ = h.rng, E = h.scale, R = (p - 0.5) * E;
          function z(Y) {
            for (var te = 0; te < g; te++) Y[m[te]] = _() * E + R;
            return Y;
          }
          if (!c) {
            var H = {};
            return d.forEachNode(function(Y) {
              H[Y] = z({});
            }), H;
          }
          d.updateEachNodeAttributes(function(Y, te) {
            return z(te), te;
          }, {
            attributes: m
          });
        }
        var u = i.bind(null, false);
        return u.assign = i.bind(null, true), yc = u, yc;
      }
      var bc, Fm;
      function IS() {
        if (Fm) return bc;
        Fm = 1;
        var l = ko(), a = Pr(), o = Math.PI / 180, i = {
          dimensions: [
            "x",
            "y"
          ],
          centeredOnZero: false,
          degrees: false
        };
        function u(d, h, m, g) {
          if (!a(h)) throw new Error("graphology-layout/rotation: the given graph is not a valid graphology instance.");
          g = l(g, i), g.degrees && (m *= o);
          var p = g.dimensions;
          if (!Array.isArray(p) || p.length !== 2) throw new Error("graphology-layout/random: given dimensions are invalid.");
          if (h.order === 0) return d ? void 0 : {};
          var _ = p[0], E = p[1], R = 0, z = 0;
          if (!g.centeredOnZero) {
            var H = 1 / 0, Y = -1 / 0, te = 1 / 0, J = -1 / 0;
            h.forEachNode(function(T, y) {
              var w = y[_], S = y[E];
              w < H && (H = w), w > Y && (Y = w), S < te && (te = S), S > J && (J = S);
            }), R = (H + Y) / 2, z = (te + J) / 2;
          }
          var I = Math.cos(m), G = Math.sin(m);
          function B(T) {
            var y = T[_], w = T[E];
            return T[_] = R + (y - R) * I - (w - z) * G, T[E] = z + (y - R) * G + (w - z) * I, T;
          }
          if (!d) {
            var Z = {};
            return h.forEachNode(function(T, y) {
              var w = {};
              w[_] = y[_], w[E] = y[E], Z[T] = B(w);
            }), Z;
          }
          h.updateEachNodeAttributes(function(T, y) {
            return B(y), y;
          }, {
            attributes: p
          });
        }
        var c = u.bind(null, false);
        return c.assign = u.bind(null, true), bc = c, bc;
      }
      var qm;
      function WS() {
        return qm || (qm = 1, Da.circlepack = QS(), Da.circular = KS(), Da.random = PS(), Da.rotation = IS()), Da;
      }
      var _c = WS();
      function JS(l) {
        if (Array.isArray(l)) return l;
      }
      function e1(l, a) {
        var o = l == null ? null : typeof Symbol < "u" && l[Symbol.iterator] || l["@@iterator"];
        if (o != null) {
          var i, u, c, d, h = [], m = true, g = false;
          try {
            if (c = (o = o.call(l)).next, a !== 0) for (; !(m = (i = c.call(o)).done) && (h.push(i.value), h.length !== a); m = true) ;
          } catch (p) {
            g = true, u = p;
          } finally {
            try {
              if (!m && o.return != null && (d = o.return(), Object(d) !== d)) return;
            } finally {
              if (g) throw u;
            }
          }
          return h;
        }
      }
      function zc(l, a) {
        (a == null || a > l.length) && (a = l.length);
        for (var o = 0, i = Array(a); o < a; o++) i[o] = l[o];
        return i;
      }
      function ap(l, a) {
        if (l) {
          if (typeof l == "string") return zc(l, a);
          var o = {}.toString.call(l).slice(8, -1);
          return o === "Object" && l.constructor && (o = l.constructor.name), o === "Map" || o === "Set" ? Array.from(l) : o === "Arguments" || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(o) ? zc(l, a) : void 0;
        }
      }
      function t1() {
        throw new TypeError(`Invalid attempt to destructure non-iterable instance.
In order to be iterable, non-array objects must have a [Symbol.iterator]() method.`);
      }
      function n1(l, a) {
        return JS(l) || e1(l, a) || ap(l, a) || t1();
      }
      function i1(l, a) {
        if (!(l instanceof a)) throw new TypeError("Cannot call a class as a function");
      }
      function a1(l, a) {
        if (typeof l != "object" || !l) return l;
        var o = l[Symbol.toPrimitive];
        if (o !== void 0) {
          var i = o.call(l, a);
          if (typeof i != "object") return i;
          throw new TypeError("@@toPrimitive must return a primitive value.");
        }
        return (a === "string" ? String : Number)(l);
      }
      function rp(l) {
        var a = a1(l, "string");
        return typeof a == "symbol" ? a : a + "";
      }
      function r1(l, a) {
        for (var o = 0; o < a.length; o++) {
          var i = a[o];
          i.enumerable = i.enumerable || false, i.configurable = true, "value" in i && (i.writable = true), Object.defineProperty(l, rp(i.key), i);
        }
      }
      function l1(l, a, o) {
        return a && r1(l.prototype, a), Object.defineProperty(l, "prototype", {
          writable: false
        }), l;
      }
      function Ao(l) {
        return Ao = Object.setPrototypeOf ? Object.getPrototypeOf.bind() : function(a) {
          return a.__proto__ || Object.getPrototypeOf(a);
        }, Ao(l);
      }
      function lp() {
        try {
          var l = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
          }));
        } catch {
        }
        return (lp = function() {
          return !!l;
        })();
      }
      function kc(l) {
        if (l === void 0) throw new ReferenceError("this hasn't been initialised - super() hasn't been called");
        return l;
      }
      function o1(l, a) {
        if (a && (typeof a == "object" || typeof a == "function")) return a;
        if (a !== void 0) throw new TypeError("Derived constructors may only return object or undefined");
        return kc(l);
      }
      function u1(l, a, o) {
        return a = Ao(a), o1(l, lp() ? Reflect.construct(a, o || [], Ao(l).constructor) : a.apply(l, o));
      }
      function Mc(l, a) {
        return Mc = Object.setPrototypeOf ? Object.setPrototypeOf.bind() : function(o, i) {
          return o.__proto__ = i, o;
        }, Mc(l, a);
      }
      function s1(l, a) {
        if (typeof a != "function" && a !== null) throw new TypeError("Super expression must either be null or a function");
        l.prototype = Object.create(a && a.prototype, {
          constructor: {
            value: l,
            writable: true,
            configurable: true
          }
        }), Object.defineProperty(l, "prototype", {
          writable: false
        }), a && Mc(l, a);
      }
      function Na(l, a, o) {
        return (a = rp(a)) in l ? Object.defineProperty(l, a, {
          value: o,
          enumerable: true,
          configurable: true,
          writable: true
        }) : l[a] = o, l;
      }
      function c1(l) {
        if (Array.isArray(l)) return zc(l);
      }
      function f1(l) {
        if (typeof Symbol < "u" && l[Symbol.iterator] != null || l["@@iterator"] != null) return Array.from(l);
      }
      function d1() {
        throw new TypeError(`Invalid attempt to spread non-iterable instance.
In order to be iterable, non-array objects must have a [Symbol.iterator]() method.`);
      }
      function wc(l) {
        return c1(l) || f1(l) || ap(l) || d1();
      }
      function Vm(l, a) {
        var o = Object.keys(l);
        if (Object.getOwnPropertySymbols) {
          var i = Object.getOwnPropertySymbols(l);
          a && (i = i.filter(function(u) {
            return Object.getOwnPropertyDescriptor(l, u).enumerable;
          })), o.push.apply(o, i);
        }
        return o;
      }
      function Ym(l) {
        for (var a = 1; a < arguments.length; a++) {
          var o = arguments[a] != null ? arguments[a] : {};
          a % 2 ? Vm(Object(o), true).forEach(function(i) {
            Na(l, i, o[i]);
          }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(l, Object.getOwnPropertyDescriptors(o)) : Vm(Object(o)).forEach(function(i) {
            Object.defineProperty(l, i, Object.getOwnPropertyDescriptor(o, i));
          });
        }
        return l;
      }
      var h1 = "relative", g1 = {
        drawLabel: void 0,
        drawHover: void 0,
        borders: [
          {
            size: {
              value: 0.1
            },
            color: {
              attribute: "borderColor"
            }
          },
          {
            size: {
              fill: true
            },
            color: {
              attribute: "color"
            }
          }
        ]
      }, m1 = "#000000";
      function v1(l) {
        var a = l.borders, o = gm(a.filter(function(u) {
          var c = u.size;
          return "fill" in c;
        }).length), i = `
precision highp float;

varying vec2 v_diffVector;
varying float v_radius;

#ifdef PICKING_MODE
varying vec4 v_color;
#else
// For normal mode, we use the border colors defined in the program:
`.concat(a.flatMap(function(u, c) {
          var d = u.size;
          return "attribute" in d ? [
            "varying float v_borderSize_".concat(c + 1, ";")
          ] : [];
        }).join(`
`), `
`).concat(a.flatMap(function(u, c) {
          var d = u.color;
          return "attribute" in d ? [
            "varying vec4 v_borderColor_".concat(c + 1, ";")
          ] : "value" in d ? [
            "uniform vec4 u_borderColor_".concat(c + 1, ";")
          ] : [];
        }).join(`
`), `
#endif

uniform float u_correctionRatio;

const float bias = 255.0 / 254.0;
const vec4 transparent = vec4(0.0, 0.0, 0.0, 0.0);

void main(void) {
  float dist = length(v_diffVector);
  float aaBorder = 2.0 * u_correctionRatio;
  float v_borderSize_0 = v_radius;
  vec4 v_borderColor_0 = transparent;

  // No antialiasing for picking mode:
  #ifdef PICKING_MODE
  if (dist > v_radius)
    gl_FragColor = transparent;
  else {
    gl_FragColor = v_color;
    gl_FragColor.a *= bias;
  }
  #else
  // Sizes:
`).concat(a.flatMap(function(u, c) {
          var d = u.size;
          if ("fill" in d) return [];
          d = d;
          var h = "attribute" in d ? "v_borderSize_".concat(c + 1) : gm(d.value), m = (d.mode || h1) === "pixels" ? "u_correctionRatio" : "v_radius";
          return [
            "  float borderSize_".concat(c + 1, " = ").concat(m, " * ").concat(h, ";")
          ];
        }).join(`
`), `
  // Now, let's split the remaining space between "fill" borders:
  float fillBorderSize = (v_radius - (`).concat(a.flatMap(function(u, c) {
          var d = u.size;
          return "fill" in d ? [] : [
            "borderSize_".concat(c + 1)
          ];
        }).join(" + "), ") ) / ").concat(o, `;
`).concat(a.flatMap(function(u, c) {
          var d = u.size;
          return "fill" in d ? [
            "  float borderSize_".concat(c + 1, " = fillBorderSize;")
          ] : [];
        }).join(`
`), `

  // Finally, normalize all border sizes, to start from the full size and to end with the smallest:
  float adjustedBorderSize_0 = v_radius;
`).concat(a.map(function(u, c) {
          return "  float adjustedBorderSize_".concat(c + 1, " = adjustedBorderSize_").concat(c, " - borderSize_").concat(c + 1, ";");
        }).join(`
`), `

  // Colors:
  vec4 borderColor_0 = transparent;
`).concat(a.map(function(u, c) {
          var d = u.color, h = [];
          return "attribute" in d ? h.push("  vec4 borderColor_".concat(c + 1, " = v_borderColor_").concat(c + 1, ";")) : "transparent" in d ? h.push("  vec4 borderColor_".concat(c + 1, " = vec4(0.0, 0.0, 0.0, 0.0);")) : h.push("  vec4 borderColor_".concat(c + 1, " = u_borderColor_").concat(c + 1, ";")), h.push("  borderColor_".concat(c + 1, ".a *= bias;")), h.push("  if (borderSize_".concat(c + 1, " <= 1.0 * u_correctionRatio) { borderColor_").concat(c + 1, " = borderColor_").concat(c, "; }")), h.join(`
`);
        }).join(`
`), `
  if (dist > adjustedBorderSize_0) {
    gl_FragColor = borderColor_0;
  } else `).concat(a.map(function(u, c) {
          return "if (dist > adjustedBorderSize_".concat(c, ` - aaBorder) {
    gl_FragColor = mix(borderColor_`).concat(c + 1, ", borderColor_").concat(c, ", (dist - adjustedBorderSize_").concat(c, ` + aaBorder) / aaBorder);
  } else if (dist > adjustedBorderSize_`).concat(c + 1, `) {
    gl_FragColor = borderColor_`).concat(c + 1, `;
  } else `);
        }).join(""), ` { /* Nothing to add here */ }
  #endif
}
`);
        return i;
      }
      function p1(l) {
        var a = l.borders, o = `
attribute vec2 a_position;
attribute float a_size;
attribute float a_angle;

uniform mat3 u_matrix;
uniform float u_sizeRatio;
uniform float u_correctionRatio;

varying vec2 v_diffVector;
varying float v_radius;

#ifdef PICKING_MODE
attribute vec4 a_id;
varying vec4 v_color;
#else
`.concat(a.flatMap(function(i, u) {
          var c = i.size;
          return "attribute" in c ? [
            "attribute float a_borderSize_".concat(u + 1, ";"),
            "varying float v_borderSize_".concat(u + 1, ";")
          ] : [];
        }).join(`
`), `
`).concat(a.flatMap(function(i, u) {
          var c = i.color;
          return "attribute" in c ? [
            "attribute vec4 a_borderColor_".concat(u + 1, ";"),
            "varying vec4 v_borderColor_".concat(u + 1, ";")
          ] : [];
        }).join(`
`), `
#endif

const float bias = 255.0 / 254.0;
const vec4 transparent = vec4(0.0, 0.0, 0.0, 0.0);

void main() {
  float size = a_size * u_correctionRatio / u_sizeRatio * 4.0;
  vec2 diffVector = size * vec2(cos(a_angle), sin(a_angle));
  vec2 position = a_position + diffVector;
  gl_Position = vec4(
    (u_matrix * vec3(position, 1)).xy,
    0,
    1
  );

  v_radius = size / 2.0;
  v_diffVector = diffVector;

  #ifdef PICKING_MODE
  v_color = a_id;
  #else
`).concat(a.flatMap(function(i, u) {
          var c = i.size;
          return "attribute" in c ? [
            "  v_borderSize_".concat(u + 1, " = a_borderSize_").concat(u + 1, ";")
          ] : [];
        }).join(`
`), `
`).concat(a.flatMap(function(i, u) {
          var c = i.color;
          return "attribute" in c ? [
            "  v_borderColor_".concat(u + 1, " = a_borderColor_").concat(u + 1, ";")
          ] : [];
        }).join(`
`), `
  #endif
}
`);
        return o;
      }
      var op = WebGLRenderingContext, $m = op.UNSIGNED_BYTE, _o = op.FLOAT;
      function up(l) {
        var a, o = Ym(Ym({}, g1), l || {}), i = o.borders, u = o.drawLabel, c = o.drawHover, d = [
          "u_sizeRatio",
          "u_correctionRatio",
          "u_matrix"
        ].concat(wc(i.flatMap(function(h, m) {
          var g = h.color;
          return "value" in g ? [
            "u_borderColor_".concat(m + 1)
          ] : [];
        })));
        return a = function(h) {
          s1(m, h);
          function m() {
            var g;
            i1(this, m);
            for (var p = arguments.length, _ = new Array(p), E = 0; E < p; E++) _[E] = arguments[E];
            return g = u1(this, m, [].concat(_)), Na(kc(g), "drawLabel", u), Na(kc(g), "drawHover", c), g;
          }
          return l1(m, [
            {
              key: "getDefinition",
              value: function() {
                return {
                  VERTICES: 3,
                  VERTEX_SHADER_SOURCE: p1(o),
                  FRAGMENT_SHADER_SOURCE: v1(o),
                  METHOD: WebGLRenderingContext.TRIANGLES,
                  UNIFORMS: d,
                  ATTRIBUTES: [
                    {
                      name: "a_position",
                      size: 2,
                      type: _o
                    },
                    {
                      name: "a_id",
                      size: 4,
                      type: $m,
                      normalized: true
                    },
                    {
                      name: "a_size",
                      size: 1,
                      type: _o
                    }
                  ].concat(wc(i.flatMap(function(p, _) {
                    var E = p.color;
                    return "attribute" in E ? [
                      {
                        name: "a_borderColor_".concat(_ + 1),
                        size: 4,
                        type: $m,
                        normalized: true
                      }
                    ] : [];
                  })), wc(i.flatMap(function(p, _) {
                    var E = p.size;
                    return "attribute" in E ? [
                      {
                        name: "a_borderSize_".concat(_ + 1),
                        size: 1,
                        type: _o
                      }
                    ] : [];
                  }))),
                  CONSTANT_ATTRIBUTES: [
                    {
                      name: "a_angle",
                      size: 1,
                      type: _o
                    }
                  ],
                  CONSTANT_DATA: [
                    [
                      m.ANGLE_1
                    ],
                    [
                      m.ANGLE_2
                    ],
                    [
                      m.ANGLE_3
                    ]
                  ]
                };
              }
            },
            {
              key: "processVisibleItem",
              value: function(p, _, E) {
                var R = this.array;
                R[_++] = E.x, R[_++] = E.y, R[_++] = p, R[_++] = E.size, i.forEach(function(z) {
                  var H = z.color;
                  "attribute" in H && (R[_++] = Hi(E[H.attribute] || H.defaultValue || m1));
                }), i.forEach(function(z) {
                  var H = z.size;
                  "attribute" in H && (R[_++] = E[H.attribute] || H.defaultValue);
                });
              }
            },
            {
              key: "setUniforms",
              value: function(p, _) {
                var E = _.gl, R = _.uniformLocations, z = R.u_sizeRatio, H = R.u_correctionRatio, Y = R.u_matrix;
                E.uniform1f(H, p.correctionRatio), E.uniform1f(z, p.sizeRatio), E.uniformMatrix3fv(Y, false, p.matrix), i.forEach(function(te, J) {
                  var I = te.color;
                  if ("value" in I) {
                    var G = R["u_borderColor_".concat(J + 1)], B = RE(I.value), Z = n1(B, 4), T = Z[0], y = Z[1], w = Z[2], S = Z[3];
                    E.uniform4f(G, T / 255, y / 255, w / 255, S / 255);
                  }
                });
              }
            }
          ]), m;
        }(Fv), Na(a, "ANGLE_1", 0), Na(a, "ANGLE_2", 2 * Math.PI / 3), Na(a, "ANGLE_3", 4 * Math.PI / 3), a;
      }
      up();
      const y1 = [
        "silver",
        "red",
        "orange",
        "gold",
        "yellowgreen",
        "green"
      ];
      var sp = ((l) => (l.circular = "circular", l.force = "force", l.random = "random", l.circlepack = "circlepack", l))(sp || {});
      function b1({ events: l, time: a, layout: o, graph: i, highlightedNode: u, setHighlightedNode: c }) {
        const d = YS(), h = VS(), m = ae.useRef(0);
        return ae.useEffect(() => {
          let g;
          a < m.current ? (i.clear(), g = 0) : (g = m.current, m.current = a);
          const p = (R) => {
            for (const z of [
              R.node,
              R.peer
            ]) {
              const H = z.toString();
              i.hasNode(H) || i.addNode(H, {
                label: H
              });
            }
          }, _ = (R) => {
            const z = R.node.toString(), H = R.peer.toString(), Y = `${z}->${H}`;
            return {
              src: z,
              dst: H,
              key: Y
            };
          }, E = l.filter((R) => R.time > g && R.time <= a);
          for (const R of E) if (p(R), R.event === "up") {
            const { src: z, dst: H, key: Y } = _(R);
            i.hasEdge(Y) || i.addDirectedEdgeWithKey(Y, z, H);
          } else if (R.event === "down") {
            const { key: z } = _(R);
            i.dropEdge(z);
          }
          if (o === "circular") _c.circular.assign(i);
          else if (o === "random") _c.random.assign(i);
          else if (o === "circlepack") _c.circlepack.assign(i);
          else throw new Error("invalid layout: " + o);
          i.forEachNode((R) => {
            const z = i.degree(R), H = y1[Math.min(z, 5)];
            i.setNodeAttribute(R, "color", H), u == R ? (i.setNodeAttribute(R, "borderColor", "fuchsia"), i.setNodeAttribute(R, "borderSize", 0.3), i.setNodeAttribute(R, "size", 12)) : u && i.areNeighbors(R, u) ? (i.setNodeAttribute(R, "borderColor", "fuchia"), i.setNodeAttribute(R, "borderSize", 0.15), i.setNodeAttribute(R, "size", 9)) : (i.setNodeAttribute(R, "borderSize", 0), i.setNodeAttribute(R, "size", 7));
          }), i.forEachEdge((R) => {
            let z;
            u ? i.extremities(R).includes(u) ? z = "fuchsia" : z = "rgba(0,0,0,0.05)" : z = "rgba(0,0,0,0.1)", i.setEdgeAttribute(R, "color", z), i.setEdgeAttribute(R, "size", 1), i.setEdgeAttribute(R, "arrow", "both");
          }), d(i);
        }, [
          i,
          a,
          l,
          o,
          d,
          u
        ]), ae.useEffect(() => {
          h({
            clickNode: ({ node: g }) => {
              c((p) => p === g ? null : g);
            }
          });
        }, [
          h,
          c
        ]), null;
      }
      const cp = xn.createContext(null);
      function _1({ children: l }) {
        const [a, o] = ae.useState(null), i = ae.useMemo(() => new Qc(), []), [u, c] = ae.useState([]), [d, h] = ae.useState(0), [m, g] = ae.useState("circular"), [p, _] = ae.useState(0), E = {
          graph: i,
          time: d,
          setTime: h,
          layout: m,
          setLayout: g,
          maxTime: p,
          setMaxTime: _,
          events: u,
          setEvents: c,
          highlightedNode: a,
          setHighlightedNode: o
        };
        return V.jsx(cp.Provider, {
          value: E,
          children: l
        });
      }
      function Mo() {
        const l = ae.useContext(cp);
        function a(o) {
          var _a;
          if (!l) throw new Error("Missing graph context");
          l.setEvents(o);
          const i = ((_a = o[o.length - 1]) == null ? void 0 : _a.time) || 0;
          l.setMaxTime(i), l.setTime(i);
        }
        return {
          ...l,
          load: a
        };
      }
      function w1() {
        const l = Mo();
        if (!l) return null;
        const { graph: a, highlightedNode: o, setHighlightedNode: i, events: u, time: c, layout: d } = l, h = ae.useMemo(() => ({
          allowInvalidContainer: true,
          renderEdgeLabels: true,
          defaultEdgeType: "straight",
          edgeProgramClasses: {
            straight: Pv({
              lengthToThicknessRatio: 5,
              widenessToThicknessRatio: 5
            })
          },
          defaultNodeType: "bordered",
          nodeProgramClasses: {
            bordered: up({
              borders: [
                {
                  size: {
                    attribute: "borderSize",
                    defaultValue: 0
                  },
                  color: {
                    attribute: "borderColor"
                  }
                },
                {
                  size: {
                    fill: true
                  },
                  color: {
                    attribute: "color"
                  }
                }
              ]
            })
          }
        }), []);
        return V.jsx($S, {
          style: {
            height: "100vh"
          },
          settings: h,
          children: u.length > 0 && V.jsx(b1, {
            events: u,
            time: c,
            layout: d,
            graph: a,
            highlightedNode: o,
            setHighlightedNode: i
          })
        });
      }
      function E1() {
        var _a;
        const l = Mo();
        if (!l) return null;
        const { graph: a, events: o, highlightedNode: i, setLayout: u, time: c, maxTime: d, setTime: h } = l, m = ((_a = o.find((p) => p.time > c)) == null ? void 0 : _a.time) || c, g = ae.useMemo(() => o.filter((p) => p.time === c), [
          o,
          c
        ]);
        return V.jsxs("div", {
          className: "sidebar",
          children: [
            V.jsx("select", {
              onChange: (p) => u(p.target.value),
              children: Object.keys(sp).map((p, _) => V.jsx("option", {
                value: p,
                children: p
              }, _))
            }),
            V.jsx("input", {
              type: "range",
              min: 0,
              max: d,
              step: 1,
              value: c,
              onChange: (p) => h(parseInt(p.target.value))
            }),
            V.jsx("button", {
              disabled: c === m,
              onClick: (p) => h(m),
              children: "Next"
            }),
            V.jsxs("div", {
              children: [
                "Time: ",
                c
              ]
            }),
            V.jsx("h3", {
              children: "Events at time"
            }),
            g.map((p, _) => V.jsxs("div", {
              children: [
                "[",
                p.time,
                "] ",
                p.node,
                " ",
                p.event,
                " ",
                p.peer
              ]
            }, _)),
            i && V.jsx(S1, {
              graph: a,
              node: i
            })
          ]
        });
      }
      function S1({ graph: l, node: a }) {
        const o = l.edges(a);
        return V.jsxs("div", {
          children: [
            V.jsxs("h3", {
              children: [
                "node ",
                a
              ]
            }),
            V.jsx("ul", {
              children: o.map((i) => V.jsx("li", {
                children: i
              }, i))
            })
          ]
        });
      }
      const Xm = (l) => typeof l == "boolean" ? `${l}` : l === 0 ? "0" : l, Zm = Jm, x1 = (l, a) => (o) => {
        var i;
        if ((a == null ? void 0 : a.variants) == null) return Zm(l, o == null ? void 0 : o.class, o == null ? void 0 : o.className);
        const { variants: u, defaultVariants: c } = a, d = Object.keys(u).map((g) => {
          const p = o == null ? void 0 : o[g], _ = c == null ? void 0 : c[g];
          if (p === null) return null;
          const E = Xm(p) || Xm(_);
          return u[g][E];
        }), h = o && Object.entries(o).reduce((g, p) => {
          let [_, E] = p;
          return E === void 0 || (g[_] = E), g;
        }, {}), m = a == null || (i = a.compoundVariants) === null || i === void 0 ? void 0 : i.reduce((g, p) => {
          let { class: _, className: E, ...R } = p;
          return Object.entries(R).every((z) => {
            let [H, Y] = z;
            return Array.isArray(Y) ? Y.includes({
              ...c,
              ...h
            }[H]) : {
              ...c,
              ...h
            }[H] === Y;
          }) ? [
            ...g,
            _,
            E
          ] : g;
        }, []);
        return Zm(l, d, m, o == null ? void 0 : o.class, o == null ? void 0 : o.className);
      }, T1 = x1("inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive", {
        variants: {
          variant: {
            default: "bg-primary text-primary-foreground shadow-xs hover:bg-primary/90",
            destructive: "bg-destructive text-white shadow-xs hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/60",
            outline: "border bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50",
            secondary: "bg-secondary text-secondary-foreground shadow-xs hover:bg-secondary/80",
            ghost: "hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50",
            link: "text-primary underline-offset-4 hover:underline"
          },
          size: {
            default: "h-9 px-4 py-2 has-[>svg]:px-3",
            sm: "h-8 rounded-md gap-1.5 px-3 has-[>svg]:px-2.5",
            lg: "h-10 rounded-md px-6 has-[>svg]:px-4",
            icon: "size-9"
          }
        },
        defaultVariants: {
          variant: "default",
          size: "default"
        }
      });
      function A1({ className: l, variant: a, size: o, asChild: i = false, ...u }) {
        const c = i ? H0 : "button";
        return V.jsx(c, {
          "data-slot": "button",
          className: Kr(T1({
            variant: a,
            size: o,
            className: l
          })),
          ...u
        });
      }
      function R1({ defaultUrl: l }) {
        const a = Mo(), [o, i] = ae.useState(l);
        function u() {
          fw.parse(o, {
            download: true,
            header: true,
            dynamicTyping: true,
            complete: (c) => {
              const d = c.data.filter((h) => h.time);
              a.load(d);
            }
          });
        }
        return V.jsxs("div", {
          className: "pb-8",
          children: [
            V.jsx("h3", {
              children: "Load simulation"
            }),
            V.jsx("input", {
              value: o,
              onChange: (c) => i(c.target.value)
            }),
            V.jsx("button", {
              onClick: (c) => u(),
              children: "Load"
            })
          ]
        });
      }
      function C1({ config: l }) {
        const a = Mo(), [o, i] = ae.useState(0), [u, c] = ae.useState(10);
        function d() {
          const m = Vb({
            seed: o,
            peers: u,
            config: l
          });
          console.log("simulator events", m);
          const g = m.events.filter((p) => p.time);
          a && a.load(g);
        }
        return V.jsxs("div", {
          className: "pb-8",
          children: [
            V.jsx("h3", {
              children: "Run simulation"
            }),
            V.jsx("label", {
              htmlFor: "peers",
              children: "Number of nodes"
            }),
            V.jsx("input", {
              name: "peers",
              type: "number",
              value: u,
              onChange: (h) => c(Number(h.target.value))
            }),
            V.jsx("br", {}),
            V.jsx("label", {
              htmlFor: "peers",
              children: "RNG seed"
            }),
            V.jsx("input", {
              name: "seed",
              type: "number",
              value: o,
              onChange: (h) => i(Number(h.target.value))
            }),
            V.jsx("br", {}),
            V.jsx(A1, {
              onClick: d,
              children: "Run"
            })
          ]
        });
      }
      function D1() {
        const [l, a] = ae.useState(() => qb().config);
        return V.jsx(_1, {
          children: V.jsxs("div", {
            className: "flex h-screen",
            children: [
              V.jsxs("div", {
                className: "w-80 h-full bg-background border-r border-border p-4 overflow-y-auto",
                children: [
                  V.jsx(R1, {
                    defaultUrl: "../data/GossipMulti-n100-r30.events.0.csv"
                  }),
                  V.jsx(C1, {
                    config: l
                  }),
                  V.jsx(ow, {
                    config: l,
                    setConfig: a
                  })
                ]
              }),
              V.jsx(w1, {}),
              V.jsx("div", {
                className: "w-80 h-full bg-background border-l border-border p-4 overflow-y-auto",
                children: V.jsx(E1, {})
              })
            ]
          })
        });
      }
      kb.createRoot(document.getElementById("root")).render(V.jsx(ae.StrictMode, {
        children: V.jsx(D1, {})
      }));
    })();
  }
});
export default require_stdin();
